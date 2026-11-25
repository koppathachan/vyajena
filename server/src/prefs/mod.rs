//! Prefs service: a user's private notification preferences (notification-prefs,
//! change `mute-notifications`).
//!
//! `Prefs.SetMutePref` records the caller's mute preference in a private
//! per-user mute set — muted conversation ids plus a `muted_all` flag — keyed by
//! the caller's username. The set is sent to that user's devices on every
//! connect as one `MutePrefs` snapshot (the `remove-closed-dm` rail, one
//! snapshot frame rather than one per entry because it carries the global flag
//! and shrinks on unmute), and the wake-push sender consults it before every
//! ping. The server stores conversation ids and the one flag only, never message
//! content or history.

use rusqlite::{Connection, params};
use tonic::{Request, Response, Status};

use crate::auth::{Caller, caller};
use crate::membership::group::GroupState;
use crate::pb;
use crate::pb::prefs_server::{Prefs, PrefsServer};
use crate::runtime::State;
use crate::store::StoreError;

/// The gRPC service.
pub fn service(state: &State) -> PrefsServer<PrefsService> {
    PrefsServer::new(PrefsService {
        state: state.clone(),
    })
}

/// Implements `vyajena.v1.Prefs`.
pub struct PrefsService {
    state: State,
}

/// Whether `username` has a device that is a current member of `group_id` (the
/// lobby, a named group they are in, or a DM they belong to). A conversation the
/// user is not in — or an unknown id — has no such member.
fn is_member_of(conn: &Connection, group_id: &[u8], username: &str) -> Result<bool, StoreError> {
    Ok(GroupState::members(conn, group_id)?
        .iter()
        .any(|m| m.username == username))
}

#[tonic::async_trait]
impl Prefs for PrefsService {
    async fn set_mute_pref(
        &self,
        request: Request<pb::SetMutePrefRequest>,
    ) -> Result<Response<pb::SetMutePrefResponse>, Status> {
        let caller = caller(&request)?.clone();
        let request = request.into_inner();
        let scope = pb::MuteScope::try_from(request.scope).unwrap_or(pb::MuteScope::Unspecified);
        let result = self
            .state
            .store
            .call(move |conn| apply(conn, &caller, scope, &request.group_id, request.muted))
            .await?;
        result?;
        Ok(Response::new(pb::SetMutePrefResponse {}))
    }
}

/// Applies one `SetMutePref` to the caller's mute set, idempotently. Returns
/// `PERMISSION_DENIED` for a conversation the caller is not a member of (so ids
/// of others' conversations are not probed) and `INVALID_ARGUMENT` for an
/// unspecified scope.
fn apply(
    conn: &mut Connection,
    caller: &Caller,
    scope: pb::MuteScope,
    group_id: &[u8],
    muted: bool,
) -> Result<Result<(), Status>, StoreError> {
    let tx = conn.transaction()?;
    match scope {
        pb::MuteScope::Conversation => {
            // notification-prefs: Mute only a conversation you are in.
            if !is_member_of(&tx, group_id, &caller.username)? {
                return Ok(Err(Status::permission_denied(
                    "not a conversation you are a member of",
                )));
            }
            if muted {
                tx.execute(
                    "INSERT INTO muted_conversations (username, group_id) VALUES (?1, ?2)
                     ON CONFLICT DO NOTHING",
                    params![caller.username, group_id],
                )?;
            } else {
                tx.execute(
                    "DELETE FROM muted_conversations WHERE username = ?1 AND group_id = ?2",
                    params![caller.username, group_id],
                )?;
            }
        }
        pb::MuteScope::Global => {
            if muted {
                tx.execute(
                    "INSERT INTO muted_all (username) VALUES (?1) ON CONFLICT DO NOTHING",
                    params![caller.username],
                )?;
            } else {
                tx.execute(
                    "DELETE FROM muted_all WHERE username = ?1",
                    params![caller.username],
                )?;
            }
        }
        pb::MuteScope::Unspecified => {
            return Ok(Err(Status::invalid_argument("scope is required")));
        }
    }
    tx.commit()?;
    tracing::info!(scope = scope.as_str_name(), muted, "set a mute preference");
    Ok(Ok(()))
}

/// A user's private mute set: the muted conversation ids and the `muted_all`
/// flag (notification-prefs: Mute preference sync).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MuteSet {
    /// The muted conversation ids.
    pub muted_group_ids: Vec<Vec<u8>>,
    /// Whole-app mute.
    pub muted_all: bool,
}

/// Reads `username`'s mute set.
pub(crate) fn mute_set(conn: &Connection, username: &str) -> Result<MuteSet, StoreError> {
    let muted_all = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM muted_all WHERE username = ?1)",
        params![username],
        |row| row.get(0),
    )?;
    let mut stmt = conn.prepare(
        "SELECT group_id FROM muted_conversations WHERE username = ?1 ORDER BY group_id",
    )?;
    let rows = stmt.query_map(params![username], |row| row.get::<_, Vec<u8>>(0))?;
    let mut muted_group_ids = Vec::new();
    for group_id in rows {
        muted_group_ids.push(group_id?);
    }
    Ok(MuteSet {
        muted_group_ids,
        muted_all,
    })
}

/// notification-prefs: Mute preference sync. The one `MutePrefs` snapshot frame
/// of `username`'s set, sent on every connect to that user's devices.
pub(crate) fn mute_frame(conn: &Connection, username: &str) -> Result<pb::ServerFrame, StoreError> {
    let set = mute_set(conn, username)?;
    Ok(pb::ServerFrame {
        frame: Some(pb::server_frame::Frame::MutePrefs(pb::MutePrefs {
            muted_group_ids: set.muted_group_ids,
            muted_all: set.muted_all,
        })),
    })
}

/// notification-prefs: A muted conversation gets no wake push. Whether a wake
/// ping for `username` in `group_id` is suppressed: `muted_all`, or `group_id`
/// in the user's muted set. When no conversation is known (`group_id` is
/// `None`, e.g. a catch-up or backlog nudge), only `muted_all` suppresses.
pub(crate) fn push_muted(
    conn: &Connection,
    username: &str,
    group_id: Option<&[u8]>,
) -> Result<bool, StoreError> {
    if conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM muted_all WHERE username = ?1)",
        params![username],
        |row| row.get::<_, bool>(0),
    )? {
        return Ok(true);
    }
    let Some(group_id) = group_id else {
        return Ok(false);
    };
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM muted_conversations WHERE username = ?1 AND group_id = ?2)",
        params![username, group_id],
        |row| row.get(0),
    )?)
}
