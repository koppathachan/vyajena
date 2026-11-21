//! Group membership: device certificates, the OpenMLS `PublicGroup` state and
//! commit checks (server-core tasks 6.2, 6.3), the server's external Remove
//! proposals and the inactivity records (tasks 8.1, 8.2), and the devices
//! waiting for their first add (flutter-app task 4.4), and the checks for a
//! re-add after an unusable Welcome (welcome-recovery).

pub mod certificate;
pub mod group;
pub mod inactivity;
pub mod joins;
pub mod named;
pub mod readd;

pub use group::{
    AcceptedCommit, CIPHERSUITE, CreateGroupError, GroupState, HandshakeRejection, MemberDevice,
    external_sender, external_sender_credential,
};
