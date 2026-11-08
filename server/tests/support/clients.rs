//! Test users and devices: identity keys, device certificates, auth keys and an
//! OpenMLS client per device, as the Rust core will have them.

use ed25519_dalek::SigningKey;
use openmls::prelude::tls_codec::{Deserialize as _, Serialize as _};
use openmls::prelude::{
    BasicCredential, Capabilities, Ciphersuite, CredentialWithKey, Extension, ExtensionType,
    Extensions, ExternalSender, GroupId, KeyPackage, LeafNodeIndex, MlsGroup, MlsGroupJoinConfig,
    MlsMessageBodyIn, MlsMessageIn, MlsMessageOut, PURE_PLAINTEXT_WIRE_FORMAT_POLICY,
    ProcessedMessageContent, SignaturePublicKey, StagedWelcome, UnknownExtension,
};
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use openmls_traits::OpenMlsProvider as _;
use prost::Message as _;
use protocol::device_identity::{DeviceId, device_id, sign_device_certificate};
use protocol::group_membership::{ADMIN_EXTENSION_TYPE, admin_extension_data};
use protocol::wire_transport::sign_auth_challenge;

use super::Check;
use super::harness::{Conn, authed};
use super::pb;

/// The one ciphersuite.
pub const CIPHERSUITE: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;

/// The server id of test servers.
pub const SERVER_ID: &str = "pi";

/// 32 random bytes.
pub fn random32() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).check("randomness");
    bytes
}

/// A fresh Ed25519 key.
pub fn new_key() -> SigningKey {
    SigningKey::from_bytes(&random32())
}

/// Leaf capabilities every device advertises: the admin extension in the
/// group context (protocol::group_membership::ADMIN_EXTENSION_TYPE).
pub fn capabilities() -> Capabilities {
    Capabilities::new(
        None,
        Some(&[CIPHERSUITE]),
        Some(&[ExtensionType::Unknown(ADMIN_EXTENSION_TYPE)]),
        None,
        None,
    )
}

/// A user: a username and the identity key their first device holds.
pub struct TestUser {
    /// The local part of the user id.
    pub username: String,
    /// The user identity key.
    pub identity: SigningKey,
}

impl TestUser {
    /// A user with a fresh identity key.
    pub fn new(username: &str) -> Self {
        Self {
            username: username.to_owned(),
            identity: new_key(),
        }
    }

    /// `<username>@pi`.
    pub fn user_id(&self) -> String {
        format!("{}@{SERVER_ID}", self.username)
    }

    /// The proto user id.
    pub fn pb_user_id(&self) -> pb::UserId {
        pb::UserId {
            username: self.username.clone(),
            server_id: SERVER_ID.to_owned(),
        }
    }

    /// The identity public key bytes.
    pub fn identity_public_key(&self) -> Vec<u8> {
        self.identity.verifying_key().to_bytes().to_vec()
    }

    /// A new device of this user, its certificate signed by the identity key.
    pub fn new_device(&self) -> TestDevice {
        TestDevice::new(&self.username, &self.identity)
    }
}

/// A device: auth key, MLS signature key, certificate, and an OpenMLS client.
pub struct TestDevice {
    /// Its user's username.
    pub username: String,
    /// The device auth key.
    pub auth: SigningKey,
    /// The device id derived from the auth key.
    pub device_id: DeviceId,
    /// OpenMLS crypto and storage.
    pub provider: OpenMlsRustCrypto,
    /// The MLS signature key pair.
    pub signer: SignatureKeyPair,
    /// The signed certificate, as sent in RPCs and the MLS credential.
    pub certificate: pb::SignedDeviceCertificate,
    /// The device's MLS group, once it has one.
    pub group: Option<MlsGroup>,
    /// The current session token.
    pub token: Option<String>,
}

/// Encodes and signs a certificate for `user`'s device.
pub fn sign_certificate(
    identity: &SigningKey,
    user: pb::UserId,
    auth_public_key: &[u8],
    mls_signature_public_key: &[u8],
) -> pb::SignedDeviceCertificate {
    let certificate = pb::DeviceCertificate {
        device: Some(pb::DeviceRef {
            user_id: Some(user),
            device_id: device_id(auth_public_key).to_vec(),
        }),
        auth_public_key: auth_public_key.to_vec(),
        mls_signature_public_key: mls_signature_public_key.to_vec(),
        created_at: Some(prost_types::Timestamp {
            seconds: 1_794_182_400,
            nanos: 0,
        }),
    }
    .encode_to_vec();
    let signature = sign_device_certificate(identity, &certificate).to_vec();
    pb::SignedDeviceCertificate {
        certificate,
        signature,
    }
}

impl TestDevice {
    /// A device of `username` whose certificate `identity` signs.
    pub fn new(username: &str, identity: &SigningKey) -> Self {
        let provider = OpenMlsRustCrypto::default();
        let signer =
            SignatureKeyPair::new(CIPHERSUITE.signature_algorithm()).check("MLS signature key");
        signer.store(provider.storage()).check("store MLS key");
        let auth = new_key();
        let auth_public = auth.verifying_key().to_bytes();
        let certificate = sign_certificate(
            identity,
            pb::UserId {
                username: username.to_owned(),
                server_id: SERVER_ID.to_owned(),
            },
            &auth_public,
            signer.public(),
        );
        Self {
            username: username.to_owned(),
            device_id: device_id(&auth_public),
            auth,
            provider,
            signer,
            certificate,
            group: None,
            token: None,
        }
    }

    /// This device as a `DeviceRef`.
    pub fn device_ref(&self) -> pb::DeviceRef {
        pb::DeviceRef {
            user_id: Some(pb::UserId {
                username: self.username.clone(),
                server_id: SERVER_ID.to_owned(),
            }),
            device_id: self.device_id.to_vec(),
        }
    }

    /// The MLS credential: a basic credential holding the encoded signed
    /// certificate (group-membership: Device-level membership).
    pub fn credential_with_key(&self) -> CredentialWithKey {
        CredentialWithKey {
            credential: BasicCredential::new(self.certificate.encode_to_vec()).into(),
            signature_key: self.signer.public().into(),
        }
    }

    /// The session token, after [`TestDevice::authenticate`].
    pub fn token(&self) -> &str {
        match &self.token {
            Some(token) => token,
            None => panic!("device has not authenticated"),
        }
    }

    /// A fresh KeyPackage, TLS-serialized.
    pub fn key_package(&self) -> Vec<u8> {
        self.key_package_with(self.credential_with_key())
    }

    /// A KeyPackage with an arbitrary credential and key (for negative tests).
    pub fn key_package_with(&self, credential: CredentialWithKey) -> Vec<u8> {
        KeyPackage::builder()
            .leaf_node_capabilities(capabilities())
            .build(CIPHERSUITE, &self.provider, &self.signer, credential)
            .check("KeyPackage")
            .key_package()
            .tls_serialize_detached()
            .check("KeyPackage bytes")
    }

    /// A KeyPackage whose leaf capabilities do not list the admin
    /// group-context extension (for negative tests).
    pub fn key_package_without_admin_capability(&self) -> Vec<u8> {
        KeyPackage::builder()
            .leaf_node_capabilities(Capabilities::new(
                None,
                Some(&[CIPHERSUITE]),
                None,
                None,
                None,
            ))
            .build(
                CIPHERSUITE,
                &self.provider,
                &self.signer,
                self.credential_with_key(),
            )
            .check("KeyPackage")
            .key_package()
            .tls_serialize_detached()
            .check("KeyPackage bytes")
    }

    /// A KeyPackage in `ciphersuite`, signed by `signer`, with `credential`
    /// (for negative tests).
    pub fn key_package_signed_by(
        &self,
        ciphersuite: Ciphersuite,
        signer: &SignatureKeyPair,
        credential: CredentialWithKey,
    ) -> Vec<u8> {
        KeyPackage::builder()
            .leaf_node_capabilities(Capabilities::new(
                None,
                Some(&[ciphersuite]),
                Some(&[ExtensionType::Unknown(ADMIN_EXTENSION_TYPE)]),
                None,
                None,
            ))
            .build(ciphersuite, &self.provider, signer, credential)
            .check("KeyPackage")
            .key_package()
            .tls_serialize_detached()
            .check("KeyPackage bytes")
    }

    /// Runs GetChallenge and Authenticate on `conn`, signing over `exporter`.
    pub async fn try_authenticate_with(
        &self,
        conn: &Conn,
        exporter: &[u8],
    ) -> Result<pb::AuthenticateResponse, tonic::Status> {
        let nonce = conn
            .devices()
            .get_challenge(pb::GetChallengeRequest {})
            .await?
            .into_inner()
            .nonce;
        self.answer(conn, &nonce, exporter).await
    }

    /// Answers the challenge `nonce` on `conn`, signing over `exporter`.
    pub async fn answer(
        &self,
        conn: &Conn,
        nonce: &[u8],
        exporter: &[u8],
    ) -> Result<pb::AuthenticateResponse, tonic::Status> {
        let signature = sign_auth_challenge(&self.auth, nonce, exporter).to_vec();
        Ok(conn
            .devices()
            .authenticate(pb::AuthenticateRequest {
                device: Some(self.device_ref()),
                nonce: nonce.to_vec(),
                signature,
            })
            .await?
            .into_inner())
    }

    /// Authenticates on `conn` and keeps the token.
    pub async fn authenticate(&mut self, conn: &Conn) {
        let response = self
            .try_authenticate_with(conn, &conn.exporter)
            .await
            .check("Authenticate");
        self.token = Some(response.session_token);
    }

    /// Uploads `one_time` fresh one-time KeyPackages plus a last-resort one.
    pub async fn upload_key_packages(&self, conn: &Conn, one_time: usize) {
        let key_packages = (0..one_time).map(|_| self.key_package()).collect();
        conn.keys()
            .upload_key_packages(authed(
                self.token(),
                pb::UploadKeyPackagesRequest {
                    key_packages,
                    last_resort_key_package: self.key_package(),
                },
            ))
            .await
            .check("UploadKeyPackages");
    }

    /// Creates the group as the admin's device would: Pi as external sender,
    /// the admin named in the group context, handshakes as PublicMessage.
    /// Returns the `CreateGroupRequest`.
    pub fn create_group(
        &mut self,
        group_id: &[u8],
        pi: &pb::ExternalSender,
        admin_user_id: &str,
    ) -> pb::CreateGroupRequest {
        let pi = external_sender(pi);
        let extensions = Extensions::from_vec(vec![
            Extension::ExternalSenders(vec![pi]),
            Extension::Unknown(
                ADMIN_EXTENSION_TYPE,
                UnknownExtension(admin_extension_data(admin_user_id)),
            ),
        ])
        .check("group context extensions");
        let group = MlsGroup::builder()
            .with_group_id(GroupId::from_slice(group_id))
            .ciphersuite(CIPHERSUITE)
            .with_wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
            .use_ratchet_tree_extension(true)
            .with_capabilities(capabilities())
            .with_group_context_extensions(extensions)
            .build(&self.provider, &self.signer, self.credential_with_key())
            .check("MlsGroup");
        self.group = Some(group);
        self.public_state()
    }

    /// The group's public state as `Admin.CreateGroup` takes it.
    pub fn public_state(&self) -> pb::CreateGroupRequest {
        let group = self.group();
        let group_info = group
            .export_group_info(self.provider.crypto(), &self.signer, false)
            .check("GroupInfo");
        pb::CreateGroupRequest {
            group_id: group.group_id().as_slice().to_vec(),
            group_info: wire(&group_info),
            ratchet_tree: group
                .export_ratchet_tree()
                .tls_serialize_detached()
                .check("ratchet tree"),
        }
    }

    /// The device's group.
    pub fn group(&self) -> &MlsGroup {
        match &self.group {
            Some(group) => group,
            None => panic!("device has no group"),
        }
    }

    /// This device's leaf.
    pub fn leaf(&self) -> LeafNodeIndex {
        self.group().own_leaf_index()
    }

    /// Commits Adds of `key_packages` (TLS-serialized). Returns the commit and
    /// Welcome; the commit stays pending until [`TestDevice::confirm`].
    pub fn add(&mut self, key_packages: &[Vec<u8>]) -> (Vec<u8>, Vec<u8>) {
        let crypto = self.provider.crypto();
        let key_packages: Vec<KeyPackage> = key_packages
            .iter()
            .map(|bytes| {
                openmls::prelude::KeyPackageIn::tls_deserialize_exact(bytes.as_slice())
                    .check("KeyPackage parse")
                    .validate(crypto, openmls::prelude::ProtocolVersion::Mls10)
                    .check("KeyPackage validate")
            })
            .collect();
        let provider = &self.provider;
        let signer = &self.signer;
        let group = match &mut self.group {
            Some(group) => group,
            None => panic!("device has no group"),
        };
        let (commit, welcome, _) = group
            .add_members(provider, signer, &key_packages)
            .check("add_members");
        (wire(&commit), wire(&welcome))
    }

    /// Commits Removes of `leaves`; pending until confirmed.
    pub fn remove(&mut self, leaves: &[LeafNodeIndex]) -> Vec<u8> {
        let provider = &self.provider;
        let signer = &self.signer;
        let group = match &mut self.group {
            Some(group) => group,
            None => panic!("device has no group"),
        };
        let (commit, _, _) = group
            .remove_members(provider, signer, leaves)
            .check("remove_members");
        wire(&commit)
    }

    /// Commits an update of its own leaf; pending until confirmed.
    pub fn self_update(&mut self) -> Vec<u8> {
        let provider = &self.provider;
        let signer = &self.signer;
        let group = match &mut self.group {
            Some(group) => group,
            None => panic!("device has no group"),
        };
        let bundle = group
            .self_update(
                provider,
                signer,
                openmls::prelude::LeafNodeParameters::default(),
            )
            .check("self_update");
        wire(bundle.commit())
    }

    /// Proposes removing the member at `leaf` (a `PROPOSAL`).
    pub fn propose_remove(&mut self, leaf: LeafNodeIndex) -> Vec<u8> {
        let provider = &self.provider;
        let signer = &self.signer;
        let group = match &mut self.group {
            Some(group) => group,
            None => panic!("device has no group"),
        };
        let (proposal, _) = group
            .propose_remove_member(provider, signer, leaf)
            .check("propose_remove_member");
        wire(&proposal)
    }

    /// Processes and stores another member's proposal.
    pub fn receive_proposal(&mut self, proposal: &[u8]) {
        let message = MlsMessageIn::tls_deserialize_exact(proposal)
            .check("proposal parse")
            .try_into_protocol_message()
            .check("protocol message");
        let provider = &self.provider;
        let group = match &mut self.group {
            Some(group) => group,
            None => panic!("device has no group"),
        };
        let processed = group.process_message(provider, message).check("process");
        match processed.into_content() {
            ProcessedMessageContent::ProposalMessage(proposal) => group
                .store_pending_proposal(provider.storage(), *proposal)
                .check("store proposal"),
            _ => panic!("not a proposal"),
        }
    }

    /// Commits all pending proposals by reference; pending until confirmed.
    pub fn commit_pending(&mut self) -> Vec<u8> {
        let provider = &self.provider;
        let signer = &self.signer;
        let group = match &mut self.group {
            Some(group) => group,
            None => panic!("device has no group"),
        };
        let (commit, _, _) = group
            .commit_to_pending_proposals(provider, signer)
            .check("commit_to_pending_proposals");
        wire(&commit)
    }

    /// The Pi accepted the pending commit.
    pub fn confirm(&mut self) {
        let provider = &self.provider;
        let group = match &mut self.group {
            Some(group) => group,
            None => panic!("device has no group"),
        };
        group
            .merge_pending_commit(provider)
            .check("merge_pending_commit");
    }

    /// The Pi rejected the pending commit.
    pub fn abort(&mut self) {
        let storage = self.provider.storage();
        let group = match &mut self.group {
            Some(group) => group,
            None => panic!("device has no group"),
        };
        group
            .clear_pending_commit(storage)
            .check("clear_pending_commit");
    }

    /// Joins from a Welcome.
    pub fn join(&mut self, welcome: &[u8]) {
        let message = MlsMessageIn::tls_deserialize_exact(welcome).check("Welcome parse");
        let MlsMessageBodyIn::Welcome(welcome) = message.extract() else {
            panic!("not a Welcome");
        };
        let config = MlsGroupJoinConfig::builder()
            .wire_format_policy(PURE_PLAINTEXT_WIRE_FORMAT_POLICY)
            .use_ratchet_tree_extension(true)
            .build();
        let staged = StagedWelcome::build_from_welcome(&self.provider, &config, welcome)
            .check("StagedWelcome")
            .replace_old_group()
            .build()
            .check("StagedWelcome build");
        self.group = Some(staged.into_group(&self.provider).check("join"));
    }

    /// The group's current epoch on this device.
    pub fn epoch(&self) -> u64 {
        self.group().epoch().as_u64()
    }

    /// Encrypts `text` as an application message (an MLS `PrivateMessage`).
    pub fn message(&mut self, text: &[u8]) -> Vec<u8> {
        let provider = &self.provider;
        let signer = &self.signer;
        let group = match &mut self.group {
            Some(group) => group,
            None => panic!("device has no group"),
        };
        wire(
            &group
                .create_message(provider, signer, text)
                .check("create_message"),
        )
    }

    /// Decrypts an application message.
    pub fn read_message(&mut self, message: &[u8]) -> Vec<u8> {
        let message = MlsMessageIn::tls_deserialize_exact(message)
            .check("message parse")
            .try_into_protocol_message()
            .check("protocol message");
        let provider = &self.provider;
        let group = match &mut self.group {
            Some(group) => group,
            None => panic!("device has no group"),
        };
        let processed = group.process_message(provider, message).check("process");
        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(message) => message.into_bytes(),
            _ => panic!("not an application message"),
        }
    }

    /// Processes a commit as a member and merges it.
    pub fn receive_commit(&mut self, commit: &[u8]) {
        let message = MlsMessageIn::tls_deserialize_exact(commit)
            .check("commit parse")
            .try_into_protocol_message()
            .check("protocol message");
        let provider = &self.provider;
        let group = match &mut self.group {
            Some(group) => group,
            None => panic!("device has no group"),
        };
        let processed = group.process_message(provider, message).check("process");
        match processed.into_content() {
            ProcessedMessageContent::StagedCommitMessage(staged) => group
                .merge_staged_commit(provider, *staged)
                .check("merge staged commit"),
            _ => panic!("not a commit"),
        }
    }
}

/// The Pi's external sender entry from `ServerInfo`.
pub fn external_sender(pi: &pb::ExternalSender) -> ExternalSender {
    let credential = openmls::prelude::Credential::tls_deserialize_exact(pi.credential.as_slice())
        .check("external sender credential");
    ExternalSender::new(
        SignaturePublicKey::from(pi.signature_public_key.clone()),
        credential,
    )
}

/// TLS-serializes an outgoing MLS message.
pub fn wire(message: &MlsMessageOut) -> Vec<u8> {
    message.tls_serialize_detached().check("MLS message bytes")
}
