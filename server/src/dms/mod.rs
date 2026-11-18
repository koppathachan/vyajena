//! Dms service: opening a DM, a 2-user MLS group next to the main group, and
//! closing DMs when a user is removed (direct-messages, change `dms`).
//!
//! Any member opens a DM with any other member; the admin has no extra power
//! here (owner, 2026-11-17). The Pi learns the pair, never the text.

use std::time::SystemTime;

use protocol::device_identity::DeviceId;
use rusqlite::{Connection, Transaction, params};
use tonic::{Request, Response, Status};

use crate::auth::{Caller, caller};
use crate::membership::group::{self, GroupState};
use crate::membership::{CreateGroupError, external_sender, joins};
use crate::pb;
use crate::pb::dms_server::{Dms, DmsServer};
use crate::runtime::State;
use crate::runtime::clock::{to_timestamp, unix_millis};
use crate::store::{StoreError, records};

/// The gRPC service.
pub fn service(state: &State) -> DmsServer<DmsService> {
    DmsServer::new(DmsService {
        state: state.clone(),
    })
}

/// Implements `vyajena.v1.Dms`.
pub struct DmsService {
    state: State,
}

/// Whether `device` is DM-capable: its last-resort KeyPackage lists the DM
/// extension (direct-messages: Old apps never join a DM).
pub fn is_dm_capable(conn: &Connection, device: &DeviceId) -> Result<bool, StoreError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM key_packages
                        WHERE device_id = ?1 AND last_resort = 1 AND dm_capable = 1)",
        params![device.as_slice()],
        |row| row.get(0),
    )?)
}

/// What `CreateDm` decided.
enum Created {
    New(Vec<(DeviceId, pb::ServerFrame)>),
    Existing(Vec<u8>),
}

fn create(
    tx: &Transaction<'_>,
    caller: &Caller,
    server_id: &str,
    pi: &openmls::prelude::ExternalSender,
    request: &pb::CreateDmRequest,
    now: SystemTime,
) -> Result<Result<Created, Status>, StoreError> {
    let Some(peer) = request.peer.as_ref() else {
        return Ok(Err(Status::invalid_argument("peer is required")));
    };
    if peer.server_id != server_id {
        return Ok(Err(Status::not_found("unknown user")));
    }
    if peer.username == caller.username {
        return Ok(Err(Status::invalid_argument("a DM needs another user")));
    }
    let known = records::user(tx, &peer.username)?.is_some_and(|user| !user.removed)
        && !records::active_devices(tx, &peer.username)?.is_empty();
    if !known {
        return Ok(Err(Status::not_found("unknown user")));
    }
    let in_main = match group::main_row(tx)? {
        Some(main) => crate::membership::inactivity::active_members(tx, &main.group_id)?
            .iter()
            .any(|(id, _)| *id == caller.device_id),
        None => false,
    };
    if !in_main {
        return Ok(Err(Status::failed_precondition(
            "only an active member of the main group opens a DM",
        )));
    }
    if !is_dm_capable(tx, &caller.device_id)? {
        return Ok(Err(Status::failed_precondition(
            "this device has no KeyPackage that lists the DM extension",
        )));
    }
    if let Some(open) = group::open_dm_between(tx, &caller.username, &peer.username)? {
        return Ok(Ok(Created::Existing(open.group_id)));
    }
    match GroupState::create_dm(tx, caller, &peer.username, server_id, pi, request, now) {
        Ok(_) => {}
        Err(CreateGroupError::Store(error)) => return Err(error),
        Err(error) => return Ok(Err(Status::from(error))),
    }
    let row = group::row(tx, &request.group_id)?
        .ok_or_else(|| StoreError::Inconsistent("a new DM without its row".into()))?;
    // direct-messages: DM members kept complete. The new DM needs the peer's
    // devices and the caller's other devices at once.
    Ok(Ok(Created::New(joins::dm_notices(tx, &row, server_id)?)))
}

#[tonic::async_trait]
impl Dms for DmsService {
    async fn create_dm(
        &self,
        request: Request<pb::CreateDmRequest>,
    ) -> Result<Response<pb::CreateDmResponse>, Status> {
        let caller = caller(&request)?.clone();
        let request = request.into_inner();
        let server_id = self.state.config.server_id.clone();
        let pi = external_sender(&server_id, &self.state.keys);
        let now = self.state.clock.now();
        let group_id = request.group_id.clone();
        let created = self
            .state
            .store
            .call(move |conn| {
                let tx = conn.transaction()?;
                let created = create(&tx, &caller, &server_id, &pi, &request, now)?;
                if matches!(created, Ok(Created::New(_))) {
                    tx.commit()?;
                }
                Ok(created)
            })
            .await??;
        match created {
            Created::New(notices) => {
                tracing::info!("opened a DM");
                for (helper, notice) in notices {
                    self.state.hub.send_frame(&helper, notice);
                }
                Ok(Response::new(pb::CreateDmResponse {
                    created: true,
                    group_id,
                }))
            }
            Created::Existing(group_id) => Ok(Response::new(pb::CreateDmResponse {
                created: false,
                group_id,
            })),
        }
    }
}

/// direct-messages: DM closed on user removal. Closes every open DM of
/// `username`: records `closed_at`, drops its queued entries, public state
/// and members, and owes a `DmClosed` to each device of the other user that
/// was a member. Returns those devices.
pub fn close_dms_of(
    tx: &Transaction<'_>,
    username: &str,
    now: SystemTime,
) -> Result<Vec<DeviceId>, StoreError> {
    let mut owed = Vec::new();
    for dm in group::open_dms(tx)? {
        let group::GroupKind::Dm { users, .. } = &dm.kind else {
            continue;
        };
        if users.0 != username && users.1 != username {
            continue;
        }
        let members = GroupState::members(tx, &dm.group_id)?;
        for member in members.iter().filter(|m| m.username != username) {
            tx.execute(
                "INSERT INTO dm_closed_notices (device_id, group_id) VALUES (?1, ?2)
                 ON CONFLICT DO NOTHING",
                params![member.device_id.as_slice(), dm.group_id],
            )?;
            owed.push(member.device_id);
        }
        tx.execute(
            "DELETE FROM deliveries WHERE server_seq IN
                 (SELECT server_seq FROM envelopes WHERE group_id = ?1)",
            params![dm.group_id],
        )?;
        tx.execute(
            "DELETE FROM envelopes WHERE group_id = ?1",
            params![dm.group_id],
        )?;
        for table in ["mls_public_state", "mls_members", "pi_removes", "gaps"] {
            tx.execute(
                &format!("DELETE FROM {table} WHERE group_id = ?1"),
                params![dm.group_id],
            )?;
        }
        tx.execute(
            "UPDATE mls_groups SET closed_at = ?2 WHERE group_id = ?1",
            params![dm.group_id, unix_millis(now)],
        )?;
        tracing::info!("closed a DM: one of its users was removed");
    }
    Ok(owed)
}

/// The `DmClosed` frames `device` still has to acknowledge.
pub(crate) fn closed_notices(
    conn: &Connection,
    device: &DeviceId,
) -> Result<Vec<pb::ServerFrame>, StoreError> {
    let mut stmt = conn.prepare(
        "SELECT n.group_id, g.closed_at FROM dm_closed_notices n
         JOIN mls_groups g ON g.group_id = n.group_id
         WHERE n.device_id = ?1 ORDER BY g.closed_at, n.group_id",
    )?;
    let rows = stmt.query_map(params![device.as_slice()], |row| {
        Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, Option<i64>>(1)?))
    })?;
    let mut frames = Vec::new();
    for row in rows {
        let (group_id, closed_at) = row?;
        frames.push(closed_frame(
            group_id,
            closed_at.map(crate::runtime::clock::from_unix_millis),
        ));
    }
    Ok(frames)
}

/// One `DmClosed` frame.
pub(crate) fn closed_frame(group_id: Vec<u8>, closed_at: Option<SystemTime>) -> pb::ServerFrame {
    pb::ServerFrame {
        frame: Some(pb::server_frame::Frame::DmClosed(pb::DmClosed {
            group_id,
            closed_at: closed_at.map(to_timestamp),
        })),
    }
}

/// The `DmClosed` frames each of `devices` is owed now, to send to the
/// connected ones at once.
pub fn open_notices(
    conn: &Connection,
    devices: &[DeviceId],
) -> Result<Vec<(DeviceId, pb::ServerFrame)>, StoreError> {
    let mut out = Vec::new();
    for device in devices {
        for frame in closed_notices(conn, device)? {
            out.push((*device, frame));
        }
    }
    out.dedup();
    Ok(out)
}

/// `device` acknowledged the `DmClosed` of `group_id`.
pub(crate) fn ack_closed(
    conn: &Connection,
    device: &DeviceId,
    group_id: &[u8],
) -> Result<(), StoreError> {
    conn.execute(
        "DELETE FROM dm_closed_notices WHERE device_id = ?1 AND group_id = ?2",
        params![device.as_slice(), group_id],
    )?;
    Ok(())
}
