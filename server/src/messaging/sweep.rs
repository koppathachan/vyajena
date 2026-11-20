//! Time-driven upkeep, run every [`SWEEP_INTERVAL`] and when tests move the
//! clock: inactivity (group-membership: Inactive devices are kept out),
//! expiry with gap notices (message-delivery: Expiry and gap notice), the
//! dedup window (message-delivery: Idempotent sends) and dropping envelopes
//! nobody needs.

use std::time::{Duration, SystemTime};

use protocol::device_identity::DeviceId;
use protocol::group_membership::INACTIVITY_TIMEOUT;
use protocol::message_delivery::{DEDUP_MIN_RETENTION, QUEUE_ENTRY_TTL};
use rusqlite::Transaction;

use super::log;
use super::write::{self, Context};
use crate::membership::{GroupState, group, inactivity};
use crate::pb;
use crate::runtime::State;
use crate::runtime::clock::to_timestamp;
use crate::store::StoreError;

/// How often the server sweeps on its own.
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(60);

struct Swept {
    notify: Vec<DeviceId>,
    gaps: Vec<(DeviceId, SystemTime, Vec<u8>)>,
}

fn before(now: SystemTime, by: Duration) -> SystemTime {
    now.checked_sub(by).unwrap_or(SystemTime::UNIX_EPOCH)
}

fn sweep_in(
    conn: &Transaction<'_>,
    ctx: &Context<'_>,
    connected: &[DeviceId],
) -> Result<Swept, StoreError> {
    let now = ctx.now;
    // Connected devices are seen now, so a long-open stream never counts as
    // absence (and survives a restart of the server).
    for device_id in connected {
        inactivity::touch(conn, device_id, now)?;
    }
    let mut notify = Vec::new();
    let mut gaps = Vec::new();
    let stale = inactivity::stale_members(conn, before(now, INACTIVITY_TIMEOUT), connected)?;
    for device_id in &stale {
        notify.extend(write::mark_inactive(conn, ctx, device_id)?);
    }
    for row in group::open_groups(conn)? {
        if inactivity::needing_proposal(conn, &row.group_id, row.epoch)?.is_empty() {
            continue;
        }
        let mut state = GroupState::load(conn, &row.group_id)?
            .ok_or_else(|| StoreError::Inconsistent("group row without state".into()))?;
        notify.extend(write::reissue_removes(conn, ctx, &mut state)?);
        state.save(conn)?;
    }
    for device_id in log::expire(conn, before(now, QUEUE_ENTRY_TTL))? {
        if connected.contains(&device_id) {
            for (group_id, at) in log::gaps(conn, &device_id)? {
                gaps.push((device_id, at, group_id));
            }
        }
    }
    log::dedup_prune(conn, before(now, DEDUP_MIN_RETENTION))?;
    log::collect_garbage(conn)?;
    notify.sort_unstable();
    notify.dedup();
    Ok(Swept { notify, gaps })
}

/// Runs one sweep at the clock's current time: marks devices inactive that
/// have not connected for 7 days, reissues the server's pending Removes, expires
/// week-old application entries (sending connected devices their gap notice
/// now; the others get it when they connect), and prunes dedup records.
pub async fn sweep(state: &State) -> Result<(), StoreError> {
    let connected: Vec<DeviceId> = state
        .hub
        .connected()
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let now = state.clock.now();
    let keys = state.keys.clone();
    let server_id = state.config.server_id.clone();
    let swept = state
        .store
        .call(move |conn| {
            let tx = conn.transaction()?;
            let ctx = Context {
                server_id: &server_id,
                keys: &keys,
                now,
            };
            let swept = sweep_in(&tx, &ctx, &connected)?;
            tx.commit()?;
            Ok(swept)
        })
        .await?;
    for (device_id, at, group_id) in swept.gaps {
        let frame = pb::ServerFrame {
            frame: Some(pb::server_frame::Frame::GapNotice(pb::GapNotice {
                group_id: group_id.clone(),
                newest_expired_at: Some(to_timestamp(at)),
            })),
        };
        if state.hub.send_frame(&device_id, frame) {
            state
                .store
                .call(move |conn| log::clear_gap(conn, &device_id, &group_id))
                .await?;
        }
    }
    super::notify(state, swept.notify);
    Ok(())
}
