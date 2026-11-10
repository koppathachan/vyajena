//! The checks OpenMLS does not do, on real OpenMLS commits: the client half
//! of commit authority and the certificate and leaf-key checks
//! (client-core task 5.2; `client::mls::verify`). The Pi refuses such
//! commits first, so these build them with OpenMLS directly and hand them to
//! the same review the engine runs before merging.

mod support;

use std::collections::BTreeMap;

use client::UserId;
use client::identity::DeviceKeys;
use client::identity::certificate;
use client::keystore::MemoryKeyStore;
use client::mls::group as mls_group;
use client::mls::verify::{self, LeafProblem};
use client::store::Store;
use ed25519_dalek::SigningKey;
use openmls::prelude::tls_codec::Deserialize as _;
use openmls::prelude::{
    BasicCredential, Credential, ExternalSender, KeyPackage, MlsGroup, MlsMessageBodyIn,
    MlsMessageIn, MlsMessageOut, ProcessedMessageContent, Sender, SignaturePublicKey, StagedCommit,
    StagedWelcome,
};
use support::Check;

const SERVER: &str = "pi";

fn seed() -> [u8; 32] {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).check("randomness");
    bytes
}

/// A device: its own store (OpenMLS state), keys and certificate.
struct Member {
    user: UserId,
    store: Store,
    keys: DeviceKeys,
    signed: Vec<u8>,
    _dir: tempfile::TempDir,
    group: Option<MlsGroup>,
}

struct User {
    id: UserId,
    identity: SigningKey,
}

impl User {
    fn new(name: &str) -> Self {
        Self {
            id: UserId::new(name, SERVER),
            identity: SigningKey::from_bytes(&seed()),
        }
    }

    fn public(&self) -> Vec<u8> {
        self.identity.verifying_key().to_bytes().to_vec()
    }

    fn device(&self) -> Member {
        let dir = support::temp_dir();
        let keys = MemoryKeyStore::new();
        let store = support::open_store(dir.path(), &keys);
        let device_keys = DeviceKeys::load_or_create(&keys).check("device keys");
        let signed = certificate::sign(
            &self.identity,
            &self.id,
            &device_keys.auth_public(),
            &device_keys.mls_public(),
            support::start_time(),
        );
        Member {
            user: self.id.clone(),
            store,
            keys: device_keys,
            signed: prost::Message::encode_to_vec(&signed),
            _dir: dir,
            group: None,
        }
    }
}

impl Member {
    fn credential(&self) -> openmls::prelude::CredentialWithKey {
        openmls::prelude::CredentialWithKey {
            credential: BasicCredential::new(self.signed.clone()).into(),
            signature_key: self.keys.mls_public().to_vec().into(),
        }
    }

    /// A KeyPackage with this device's certificate, signed by `signer_of`
    /// (normally itself; another device for the wrong-key case).
    fn key_package_signed_by(&self, signer_of: &Member) -> KeyPackage {
        let credential = openmls::prelude::CredentialWithKey {
            credential: BasicCredential::new(self.signed.clone()).into(),
            signature_key: signer_of.keys.mls_public().to_vec().into(),
        };
        KeyPackage::builder()
            .leaf_node_capabilities(mls_group::capabilities())
            .build(
                mls_group::CIPHERSUITE,
                self.store.mls(),
                &signer_of.keys.mls_signer(),
                credential,
            )
            .check("KeyPackage")
            .key_package()
            .clone()
    }

    fn key_package(&self) -> KeyPackage {
        self.key_package_signed_by(self)
    }

    /// Commits Adds of `key_packages` and merges at once (as if accepted).
    fn add(&mut self, key_packages: Vec<KeyPackage>) -> (Vec<u8>, Vec<u8>) {
        let signer = self.keys.mls_signer();
        let provider = self.store.mls();
        let group = self.group.as_mut().check("group");
        let (commit, welcome, _) = group
            .add_members(provider, &signer, &key_packages)
            .check("add_members");
        group.merge_pending_commit(provider).check("merge");
        (wire(&commit), wire(&welcome))
    }

    fn join(&mut self, welcome: &[u8]) {
        let MlsMessageBodyIn::Welcome(welcome) = MlsMessageIn::tls_deserialize_exact(welcome)
            .check("welcome")
            .extract()
        else {
            panic!("not a Welcome");
        };
        let staged =
            StagedWelcome::build_from_welcome(self.store.mls(), &mls_group::join_config(), welcome)
                .check("staged welcome")
                .build()
                .check("build");
        self.group = Some(staged.into_group(self.store.mls()).check("join"));
    }

    /// Processes another member's commit up to the staged commit.
    fn stage(&mut self, commit: &[u8]) -> (StagedCommit, u32) {
        let message = MlsMessageIn::tls_deserialize_exact(commit)
            .check("commit")
            .try_into_protocol_message()
            .check("protocol message");
        let provider = self.store.mls();
        let group = self.group.as_mut().check("group");
        let processed = group.process_message(provider, message).check("process");
        let Sender::Member(leaf) = *processed.sender() else {
            panic!("not from a member");
        };
        let ProcessedMessageContent::StagedCommitMessage(staged) = processed.into_content() else {
            panic!("not a commit");
        };
        (*staged, leaf.u32())
    }

    fn merge(&mut self, staged: StagedCommit) {
        let provider = self.store.mls();
        self.group
            .as_mut()
            .check("group")
            .merge_staged_commit(provider, staged)
            .check("merge");
    }
}

fn wire(message: &MlsMessageOut) -> Vec<u8> {
    mls_group::wire(message).check("wire")
}

fn pi() -> ExternalSender {
    ExternalSender::new(
        SignaturePublicKey::from(
            SigningKey::from_bytes(&seed())
                .verifying_key()
                .to_bytes()
                .to_vec(),
        ),
        Credential::from(BasicCredential::new(SERVER.as_bytes().to_vec())),
    )
}

/// The admin's group with Carol (who reviews) and Alice in it.
struct Setup {
    users: BTreeMap<UserId, Vec<u8>>,
    alice_user: User,
    admin: Member,
    alice: Member,
    carol: Member,
    pi: ExternalSender,
}

fn setup() -> Setup {
    let (akhil, alice_user, carol_user) =
        (User::new("akhil"), User::new("alice"), User::new("carol"));
    let mut admin = akhil.device();
    let mut alice = alice_user.device();
    let mut carol = carol_user.device();
    let pi = pi();
    admin.group = Some(
        mls_group::create(
            admin.store.mls(),
            &admin.keys.mls_signer(),
            admin.credential(),
            mls_group::new_group_id().check("group id"),
            pi.clone(),
            "akhil@pi",
        )
        .check("create"),
    );
    let (_, welcome) = admin.add(vec![alice.key_package(), carol.key_package()]);
    alice.join(&welcome);
    carol.join(&welcome);
    let users = [&akhil, &alice_user, &carol_user]
        .iter()
        .map(|u| (u.id.clone(), u.public()))
        .collect();
    Setup {
        users,
        alice_user,
        admin,
        alice,
        carol,
        pi,
    }
}

fn review(
    setup: &mut Setup,
    commit: &[u8],
    users: &BTreeMap<UserId, Vec<u8>>,
) -> Result<verify::CommitReview, LeafProblem> {
    let (staged, committer) = setup.carol.stage(commit);
    let group = setup.carol.group.as_ref().check("group");
    let pi_index = mls_group::pi_sender_index(group.extensions(), &setup.pi);
    let result = verify::review_commit(
        group,
        &staged,
        openmls::prelude::LeafNodeIndex::new(committer),
        SERVER,
        pi_index,
        &|user| users.get(user).cloned(),
    );
    if result.is_ok() {
        setup.carol.merge(staged);
    }
    result
}

// spec: group-membership/commit-authority
#[test]
fn clients_reject_a_member_adding_someone_else() {
    let mut setup = setup();
    let bob = User::new("bob");
    let bob_phone = bob.device();
    let mut users = setup.users.clone();
    users.insert(bob.id.clone(), bob.public());

    // Alice's device commits an Add of Bob's phone: OpenMLS allows it, the
    // rule does not.
    let (commit, _) = setup.alice.add(vec![bob_phone.key_package()]);
    let refused = review(&mut setup, &commit, &users);
    assert!(
        matches!(refused, Err(LeafProblem::Authority(_))),
        "{refused:?}"
    );
}

// spec: group-membership/commit-authority
// spec: group-membership/device-level-membership
#[test]
fn clients_accept_the_admin_adding_a_user_and_a_member_adding_their_own_device() {
    let mut setup = setup();
    let bob = User::new("bob");
    let bob_phone = bob.device();
    let mut users = setup.users.clone();
    users.insert(bob.id.clone(), bob.public());
    let (commit, _) = setup.admin.add(vec![bob_phone.key_package()]);
    let review_result = review(&mut setup, &commit, &users).check("admin adds bob");
    assert_eq!(
        review_result.added,
        vec![(bob.id.clone(), bob_phone_id(&bob_phone))]
    );
    setup.alice.stage_and_merge(&commit);

    // Alice links a tablet: her own device, so she may add it.
    let tablet = setup.alice_user.device();
    let (commit, _) = setup.alice.add(vec![tablet.key_package()]);
    let review_result = review(&mut setup, &commit, &users).check("alice adds her tablet");
    assert_eq!(
        review_result.added,
        vec![(setup.alice.user.clone(), tablet.keys.device_id())]
    );
}

fn bob_phone_id(member: &Member) -> [u8; 16] {
    member.keys.device_id()
}

impl Member {
    fn stage_and_merge(&mut self, commit: &[u8]) {
        let (staged, _) = self.stage(commit);
        self.merge(staged);
    }
}

// spec: group-membership/device-level-membership
#[test]
fn clients_reject_a_genuine_certificate_on_the_wrong_key() {
    let mut setup = setup();
    let bob = User::new("bob");
    let bob_phone = bob.device();
    let other = bob.device();
    let mut users = setup.users.clone();
    users.insert(bob.id.clone(), bob.public());
    // Bob's phone's certificate, with another device's signature key.
    let (commit, _) = setup
        .admin
        .add(vec![bob_phone.key_package_signed_by(&other)]);
    let refused = review(&mut setup, &commit, &users);
    assert_eq!(refused, Err(LeafProblem::WrongKey));
}

// spec: key-directory/clients-verify-the-identity-chain
// spec: group-membership/device-level-membership
#[test]
fn clients_reject_a_certificate_that_does_not_verify_against_the_pinned_key() {
    let mut setup = setup();
    let bob = User::new("bob");
    let bob_phone = bob.device();
    // Carol pinned a different key for Bob.
    let mut users = setup.users.clone();
    users.insert(bob.id.clone(), User::new("bob").public());
    let (commit, _) = setup.admin.add(vec![bob_phone.key_package()]);
    let refused = review(&mut setup, &commit, &users);
    assert_eq!(refused, Err(LeafProblem::Unverified("bob@pi".into())));

    // And with nothing pinned at all the leaf is not accepted either.
    let mut setup = self::setup();
    let users = setup.users.clone();
    let (commit, _) = setup.admin.add(vec![bob_phone.key_package()]);
    let refused = review(&mut setup, &commit, &users);
    assert_eq!(refused, Err(LeafProblem::NotPinned("bob@pi".into())));
}

/// What Carol decides about `commit` with `users` pinned: the review, and
/// whether she may look up the key of a user whose certificate failed.
fn review_with_lookup(
    setup: &mut Setup,
    commit: &[u8],
    users: &BTreeMap<UserId, Vec<u8>>,
    new_keys: &BTreeMap<UserId, Vec<u8>>,
) -> Result<verify::CommitReview, LeafProblem> {
    let (staged, committer) = setup.carol.stage(commit);
    let committer = openmls::prelude::LeafNodeIndex::new(committer);
    let group = setup.carol.group.as_ref().check("group");
    let pi_index = mls_group::pi_sender_index(group.extensions(), &setup.pi);
    let mut pinned = users.clone();
    loop {
        let result = verify::review_commit(group, &staged, committer, SERVER, pi_index, &|user| {
            pinned.get(user).cloned()
        });
        let problem = match result {
            Ok(review) => return Ok(review),
            Err(problem) => problem,
        };
        let Some(user) = verify::key_lookup_allowed(group, committer, &problem) else {
            return Err(problem);
        };
        // The Pi's answer to `Keys.GetIdentityKeys`.
        match new_keys.get(&user) {
            Some(key) if pinned.get(&user) != Some(key) => {
                pinned.insert(user, key.clone());
            }
            _ => return Err(problem),
        }
    }
}

// spec: key-directory/clients-verify-the-identity-chain
// spec: group-membership/device-level-membership
#[test]
fn an_admin_commit_with_a_reset_users_new_key_passes_after_a_lookup() {
    let mut setup = setup();
    let old_bob = User::new("bob");
    let new_bob = User::new("bob");
    let new_phone = new_bob.device();
    // Carol still has Bob's old key pinned; the Pi now has his new one.
    let mut users = setup.users.clone();
    users.insert(old_bob.id.clone(), old_bob.public());
    let new_keys: BTreeMap<UserId, Vec<u8>> = [(new_bob.id.clone(), new_bob.public())]
        .into_iter()
        .collect();

    let (commit, _) = setup.admin.add(vec![new_phone.key_package()]);
    let accepted =
        review_with_lookup(&mut setup, &commit, &users, &new_keys).check("admin re-adds bob");
    assert_eq!(
        accepted.added,
        vec![(new_bob.id.clone(), new_phone.keys.device_id())]
    );

    // A lookup that returns the old key again (a forged certificate) does
    // not help.
    let mut setup = self::setup();
    let forged = new_bob.device();
    let mut users = setup.users.clone();
    users.insert(old_bob.id.clone(), old_bob.public());
    let same: BTreeMap<UserId, Vec<u8>> = [(old_bob.id.clone(), old_bob.public())]
        .into_iter()
        .collect();
    let (commit, _) = setup.admin.add(vec![forged.key_package()]);
    let refused = review_with_lookup(&mut setup, &commit, &users, &same);
    assert_eq!(refused, Err(LeafProblem::Unverified("bob@pi".into())));
}

// spec: key-directory/clients-verify-the-identity-chain
#[test]
fn a_non_admin_commit_with_a_changed_key_is_refused_without_a_lookup() {
    let mut setup = setup();
    // Alice's tablet is certified by a new identity key of Alice, which the
    // Pi would confirm; Alice's own device commits its Add.
    let new_alice = User::new("alice");
    let tablet = new_alice.device();
    let users = setup.users.clone();
    let new_keys: BTreeMap<UserId, Vec<u8>> = [(new_alice.id.clone(), new_alice.public())]
        .into_iter()
        .collect();
    let (commit, _) = setup.alice.add(vec![tablet.key_package()]);
    let refused = review_with_lookup(&mut setup, &commit, &users, &new_keys);
    assert_eq!(refused, Err(LeafProblem::Unverified("alice@pi".into())));
}
