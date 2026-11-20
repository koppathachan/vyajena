//! multi-group: Closing a named group (change `lobby-multi-group`, design D6).

use std::time::SystemTime;

use protocol::device_identity::DeviceId;
use rusqlite::{Transaction, params};
use tonic::Status;

use crate::store::StoreError;

use super::group::{self, GroupKind, GroupState};

/// `Admin.CloseGroup`: closes the named group `group_id`: records `closed_at`,
/// drops its queued entries, public state and members, and owes a `GroupClosed`
/// to each device that was a member. Returns those devices (none if the group
/// was already closed). `FAILED_PRECONDITION` for the system group, `NOT_FOUND`
/// for an unknown group or a DM.
pub fn close_named_group(
    tx: &Transaction<'_>,
    group_id: &[u8],
    now: SystemTime,
) -> Result<Result<Vec<DeviceId>, Status>, StoreError> {
    let Some(row) = group::row(tx, group_id)? else {
        return Ok(Err(Status::not_found("unknown group")));
    };
    match row.kind {
        GroupKind::System => Ok(Err(Status::failed_precondition(
            "the system lobby cannot be closed",
        ))),
        GroupKind::Dm { .. } => Ok(Err(Status::not_found("unknown group"))),
        GroupKind::Named {
            closed_at: Some(_), ..
        } => Ok(Ok(Vec::new())),
        GroupKind::Named { closed_at: None } => {
            let mut owed = Vec::new();
            for member in GroupState::members(tx, group_id)? {
                tx.execute(
                    "INSERT INTO dm_closed_notices (device_id, group_id) VALUES (?1, ?2)
                     ON CONFLICT DO NOTHING",
                    params![member.device_id.as_slice(), group_id],
                )?;
                owed.push(member.device_id);
            }
            crate::dms::drop_group_state(tx, group_id, now)?;
            Ok(Ok(owed))
        }
    }
}
