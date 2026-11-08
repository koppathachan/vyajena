//! Group membership: device certificates, the OpenMLS `PublicGroup` state and
//! commit checks (pi-server-core tasks 6.2, 6.3); inactivity and re-add notices
//! come with tasks 8.1 and 8.2.

pub mod certificate;
pub mod group;

pub use group::{
    AcceptedCommit, CIPHERSUITE, CreateGroupError, GroupState, HandshakeRejection, MemberDevice,
    external_sender, external_sender_credential,
};
