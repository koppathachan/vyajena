//! Time-driven upkeep, run every [`SWEEP_INTERVAL`] and when tests move the
//! clock: inactivity (group-membership: Inactive devices are kept out),
//! expiry with gap notices (message-delivery: Expiry and gap notice), the
//! dedup window (message-delivery: Idempotent sends) and dropping envelopes
//! nobody needs.

use std::time::{Duration, SystemTime};

use protocol::device_identity::DeviceId;
use rusqlite::Transaction;

use super::log;
use super::write::{self, Context};
use crate::membership::{GroupState, group, inactivity};
use crate::pb;
use crate::runtime::State;
use crate::runtime::clock::to_timestamp;
use crate::store::{StoreError, records};

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
    let stale =
        inactivity::stale_members(conn, before(now, ctx.limits.inactivity_timeout), connected)?;
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
    for device_id in log::expire(conn, before(now, ctx.limits.queue_entry_ttl))? {
        if connected.contains(&device_id) {
            for (group_id, at) in log::gaps(conn, &device_id)? {
                gaps.push((device_id, at, group_id));
            }
        }
    }
    log::dedup_prune(conn, before(now, ctx.limits.dedup_min_retention))?;
    log::collect_garbage(conn)?;
    // notification-reliability: Sweep catch-up wake ping (D2) with the durable
    // attempt counter (D5). First reset the counter for devices that have
    // drained or expired their backlog, or are connected now (receiving on the
    // stream); then nudge each offline device that still has a backlog and a
    // push token, up to CATCH_UP_MAX pings, bumping its durable counter each
    // time. A device that drains or reconnects starts a fresh budget. The push
    // itself is rate-limited (one per device per 60 s) by `super::notify`.
    log::prune_catchup(conn, connected)?;
    for device_id in log::backlogged_devices(conn)? {
        if connected.contains(&device_id) {
            continue;
        }
        if records::push_token(conn, &device_id)?.is_none() {
            continue;
        }
        if log::catchup_attempts(conn, &device_id)? >= ctx.limits.catch_up_max {
            continue;
        }
        log::bump_catchup(conn, &device_id)?;
        notify.push(device_id);
    }
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
    let limits = state.config.limits;
    let swept = state
        .store
        .call(move |conn| {
            let tx = conn.transaction()?;
            let ctx = Context {
                server_id: &server_id,
                keys: &keys,
                now,
                limits,
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
