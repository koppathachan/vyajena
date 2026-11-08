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
use crate::membership::{GroupState, inactivity};
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
    // absence (and survives a restart of the Pi).
    for device_id in connected {
        inactivity::touch(conn, device_id, now)?;
    }
    let mut notify = Vec::new();
    let mut gaps = Vec::new();
    if let Some((group_id, epoch)) = log::group_row(conn)? {
        let stale = inactivity::stale_members(conn, before(now, INACTIVITY_TIMEOUT), connected)?;
        if !stale.is_empty() || !inactivity::needing_proposal(conn, epoch)?.is_empty() {
            let mut state = GroupState::load(conn)?
                .ok_or_else(|| StoreError::Inconsistent("group row without state".into()))?;
            for device_id in &stale {
                notify.extend(write::mark_inactive(conn, ctx, &mut state, device_id)?);
            }
            notify.extend(write::reissue_removes(conn, ctx, &mut state)?);
            state.save(conn)?;
        }
        for device_id in log::expire(conn, before(now, QUEUE_ENTRY_TTL))? {
            if connected.contains(&device_id)
                && let Some(at) = log::gap(conn, &device_id)?
            {
                gaps.push((device_id, at, group_id.clone()));
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
/// have not connected for 7 days, reissues the Pi's pending Removes, expires
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
                group_id,
                newest_expired_at: Some(to_timestamp(at)),
            })),
        };
        if state.hub.send_frame(&device_id, frame) {
            state
                .store
                .call(move |conn| log::clear_gap(conn, &device_id))
                .await?;
        }
    }
    super::notify(state, swept.notify);
    Ok(())
}
