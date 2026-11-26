//! [`Core`]: the app's one handle on the device core (flutter-app design D2).
//!
//! Every method forwards to [`client::Client`]; see its docs for what each
//! does. Methods are async in Dart: frb runs them on its tokio runtime (the
//! one runtime per app process, spike 1.3) or its thread pool.

use client::keystore::{KeyStoreConfig, open_key_store};
use client::{ChatId, Client, ClientConfig, EventStream, MessageId, MuteTarget, Presence, UserId};
use tokio::sync::Mutex;

use super::error::{CoreError, CoreErrorKind};
use super::text;
use super::types::{
    ChatMessage, ChatSummary, CoreEvent, InviteInfo, MemberDevice, MentionSpan, PresenceStatus,
    ProfileInfo, PushPlatform, StickerSource, UserRef,
};
use crate::frb_generated::StreamSink;
use crate::map;

/// The key store profile of the app: one user per app install.
pub const KEY_STORE_PROFILE: &str = "app";

/// The device core: one per app process (D2), held by Dart's `CoreService`.
#[flutter_rust_bridge::frb(opaque)]
pub struct Core {
    client: Client,
    events: Mutex<Option<EventStream>>,
}

impl std::fmt::Debug for Core {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Core").finish_non_exhaustive()
    }
}

/// A file path as the text Dart takes it as.
fn path_text(path: std::path::PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

fn forward<T>(result: Result<T, client::ClientError>) -> Result<T, CoreError> {
    result.map_err(map::error)
}

/// Whether `text` fits in one message (app-ui: Composer limit): encoded, it
/// is within the core's `MAX_CONTENT_BYTES`.
#[flutter_rust_bridge::frb(sync)]
pub fn text_fits(text: String) -> bool {
    client::messaging::encode_text(&text, std::time::SystemTime::now()).is_ok()
}

/// The blur a ThumbHash stands for, as a small PNG, or `None` for bytes that
/// are not a ThumbHash. The core decodes it so the app has no second ThumbHash
/// implementation.
#[flutter_rust_bridge::frb(sync)]
pub fn thumbhash_preview(thumbhash: Vec<u8>) -> Option<Vec<u8>> {
    client::media::encode::thumbhash_preview(&thumbhash)
}

/// Whether a ThumbHash encodes a transparent image. The app renders such an
/// image — a sticker — without an opaque bubble fill so its transparency reads
/// (photo-sticker: Sticker is an image; app-ui: Create a sticker). `false` for
/// bytes that are not a ThumbHash. The core decodes it.
#[flutter_rust_bridge::frb(sync)]
pub fn thumbhash_is_transparent(thumbhash: Vec<u8>) -> bool {
    client::media::encode::thumbhash_is_transparent(&thumbhash)
}

impl Core {
    /// Opens the core: the database in `data_dir` (the app's private
    /// storage) and the keys in the OS key store. The decrypted pictures
    /// are cached in `data_dir/media`: `data_dir` is the folder the platform
    /// keeps out of backups and encrypts at rest (app-ui: Data on the phone),
    /// so the cache is too.
    pub async fn open(data_dir: String) -> Result<Core, CoreError> {
        let key_store = tokio::task::spawn_blocking(|| {
            open_key_store(&KeyStoreConfig {
                profile: KEY_STORE_PROFILE.to_owned(),
                key_file: None,
            })
        })
        .await
        .map_err(|e| CoreError::new(CoreErrorKind::Internal, e.to_string()))?
        .map_err(|e| map::error(e.into()))?;
        Self::open_with(ClientConfig::new(data_dir, key_store)).await
    }

    /// Opens the core with `config` (tests: a memory key store and a manual
    /// clock). Not exposed to Dart.
    #[flutter_rust_bridge::frb(ignore)]
    pub async fn open_with(config: ClientConfig) -> Result<Core, CoreError> {
        let mut client = forward(Client::open(config).await)?;
        let events = client.take_events();
        Ok(Core {
            client,
            events: Mutex::new(events),
        })
    }

    /// The one ordered event stream (client-runtime: Events in order). Only
    /// the first call gets it; later calls fail with
    /// [`CoreErrorKind::EventsTaken`].
    pub async fn events(&self, sink: StreamSink<CoreEvent>) -> Result<(), CoreError> {
        let mut stream = self.take_stream().await?;
        tokio::spawn(async move {
            while let Some(event) = stream.next().await {
                if sink.add(map::event(event)).is_err() {
                    break;
                }
            }
        });
        Ok(())
    }

    /// The next event, for Rust callers (the bridge tests); Dart uses
    /// [`Core::events`]. `None` once the stream was taken by `events` or the
    /// core is gone.
    #[flutter_rust_bridge::frb(ignore)]
    pub async fn next_event(&self) -> Option<CoreEvent> {
        let mut events = self.events.lock().await;
        events.as_mut()?.next().await.map(map::event)
    }

    async fn take_stream(&self) -> Result<EventStream, CoreError> {
        self.events
            .lock()
            .await
            .take()
            .ok_or_else(|| CoreError::new(CoreErrorKind::EventsTaken, "the event stream is taken"))
    }

    // --- Account ---

    /// This device's user, once registered or linked.
    pub fn user_id(&self) -> Result<Option<UserRef>, CoreError> {
        Ok(forward(self.client.user_id())?.as_ref().map(map::user))
    }

    /// This device's id (hex), once registered or linked.
    pub fn device_id(&self) -> Result<Option<String>, CoreError> {
        Ok(forward(self.client.device_id())?.map(|d| map::hex(&d)))
    }

    /// Whether this device's user is the admin.
    pub fn is_admin(&self) -> Result<bool, CoreError> {
        forward(self.client.is_admin())
    }

    /// Signs up this device from an invite text (app-ui: Join from an
    /// invite). Call [`Core::start`] next.
    pub async fn register(&self, invite_text: String) -> Result<UserRef, CoreError> {
        let invite = text::invite_bytes(&invite_text)?;
        Ok(map::user(&forward(self.client.register(&invite).await)?))
    }

    /// New device, step 1: the link request text to show (app-ui: Link a
    /// device).
    pub async fn link_request(&self) -> Result<String, CoreError> {
        Ok(text::link_request_text(&forward(
            self.client.link_request().await,
        )?))
    }

    /// Identity device: approves a link request text and returns the
    /// approval text to show back.
    pub async fn approve_link(&self, request_text: String) -> Result<String, CoreError> {
        let request = text::link_request_bytes(&request_text)?;
        Ok(text::link_approval_text(&forward(
            self.client.approve_link(&request).await,
        )?))
    }

    /// New device, step 2: completes linking with the approval text. Call
    /// [`Core::start`] next.
    pub async fn complete_link(&self, approval_text: String) -> Result<UserRef, CoreError> {
        let approval = text::link_approval_bytes(&approval_text)?;
        Ok(map::user(&forward(
            self.client.complete_link(&approval).await,
        )?))
    }

    // --- Running (app-ui: Foreground and background) ---

    /// Starts (or restarts) the engine: connect, catch up, stay connected.
    pub async fn start(&self) -> Result<(), CoreError> {
        forward(self.client.start().await)
    }

    /// Stops the engine and closes the connection.
    pub fn stop(&self) {
        self.client.stop();
    }

    /// The network is back (a connectivity change): connect now instead of
    /// waiting out the backoff; does nothing while connected or after
    /// `Removed` (app-ui: Foreground and background).
    pub fn reconnect_now(&self) {
        self.client.reconnect_now();
    }

    /// Whether the core is connected right now.
    pub fn is_connected(&self) -> bool {
        self.client.is_connected()
    }

    /// Re-sync a chat the core signalled is out of sync (`CoreEvent::OutOfSync`):
    /// ask the server to re-add this wedged device. The held message keeps its
    /// id and is sent once a device able to re-add this one is online (app-ui:
    /// Out-of-sync re-sync affordance).
    pub fn request_resync(&self) {
        self.client.request_resync();
    }

    // --- Push (app-ui: Push notifications) ---

    /// Registers or replaces this device's push token on the server (needs a
    /// connection; fails `offline` otherwise, and the app retries on the next
    /// connect).
    pub async fn set_push_token(
        &self,
        platform: PushPlatform,
        token: String,
    ) -> Result<(), CoreError> {
        let platform = match platform {
            PushPlatform::Fcm => client::PushPlatform::Fcm,
            PushPlatform::Apns => client::PushPlatform::Apns,
        };
        forward(
            self.client
                .set_push_token(Some(client::PushToken { platform, token }))
                .await,
        )
    }

    /// Clears this device's push token on the server: no more wake pings
    /// (notifications not allowed).
    pub async fn clear_push_token(&self) -> Result<(), CoreError> {
        forward(self.client.set_push_token(None).await)
    }

    // --- Admin (app-ui: Admin screens) ---

    /// Admin: creates the group.
    pub async fn create_group(&self) -> Result<(), CoreError> {
        forward(self.client.create_group().await)
    }

    /// Admin: adds all devices of `user` to the group.
    pub async fn add_user(&self, user: UserRef) -> Result<(), CoreError> {
        forward(self.client.add_user(&map::user_id(&user)).await)
    }

    /// Admin: removes `user` from the group and the server.
    pub async fn remove_user(&self, user: UserRef) -> Result<(), CoreError> {
        forward(self.client.remove_user(&map::user_id(&user)).await)
    }

    /// Admin: a new invite for `username`, with its text to share.
    pub async fn create_invite(&self, username: String) -> Result<InviteInfo, CoreError> {
        Ok(map::invite(forward(
            self.client.create_invite(&username).await,
        )?))
    }

    /// Admin: the unused invites (without text).
    pub async fn list_invites(&self) -> Result<Vec<InviteInfo>, CoreError> {
        Ok(forward(self.client.list_invites().await)?
            .into_iter()
            .map(map::invite)
            .collect())
    }

    /// Admin: revokes an unused invite by its code.
    pub async fn revoke_invite(&self, code: String) -> Result<(), CoreError> {
        forward(self.client.revoke_invite(&code).await)
    }

    // --- Own devices and keys ---

    /// Revokes another device of this user (hex id).
    pub async fn revoke_device(&self, device: String) -> Result<(), CoreError> {
        let device = map::device_id(&device)?;
        forward(self.client.revoke_device(&device).await)
    }

    /// account-deletion: User deletes their own account. Calls the self-service
    /// delete RPC and, on an OK response, wipes this device — the local store
    /// and the keystore keys — so the core returns to a no-account state and the
    /// app shows onboarding (account-deletion: The initiating device wipes
    /// itself on success). Wipes nothing on failure, and refuses on the admin's
    /// device (account-deletion: The admin cannot self-delete through the RPC).
    pub async fn delete_account(&self) -> Result<(), CoreError> {
        forward(self.client.delete_account().await)
    }

    /// The admin accepted `user`'s changed identity key (app-ui: Notices in
    /// the chat). `false` if no new key is pending.
    pub fn accept_identity_key(&self, user: UserRef) -> Result<bool, CoreError> {
        forward(self.client.accept_identity_key(&map::user_id(&user)))
    }

    /// The owner acknowledges `user`'s changed safety number (admin-recovery:
    /// Admin re-key forces peer re-trust): the re-trust prompt is cleared and
    /// the identity is verified again. On admin devices a key kept waiting is
    /// also accepted and the user re-added. `false` if nothing was pending.
    pub fn acknowledge_identity_key(&self, user: UserRef) -> Result<bool, CoreError> {
        forward(self.client.acknowledge_identity_key(&map::user_id(&user)))
    }

    /// The users whose safety number changed and await the owner's re-trust
    /// (admin-recovery: Admin re-key forces peer re-trust). The app shows a
    /// prompt for each and withholds the verified state until acknowledged.
    pub fn identity_retrust_pending(&self) -> Result<Vec<UserRef>, CoreError> {
        let users = forward(self.client.identity_retrust_pending())?;
        Ok(users.iter().map(map::user).collect())
    }

    // --- Messaging (app-ui: Chat screen, Profile and members) ---

    /// Sends a text message; returns its local id. Its status arrives as
    /// [`CoreEvent::SendStatus`].
    pub fn send_text(&self, text: String) -> Result<i64, CoreError> {
        Ok(forward(self.client.send_text(&text))?.0)
    }

    /// Sends a text message with `mentions` (UTF-16 ranges of `text`),
    /// answering the message with local id `reply_to` if set (message-content:
    /// Mentions; Replies); returns its local id.
    pub fn send_message(
        &self,
        text: String,
        mentions: Vec<MentionSpan>,
        reply_to: Option<i64>,
    ) -> Result<i64, CoreError> {
        let mentions = map::mentions(&text, &mentions)?;
        Ok(forward(
            self.client
                .send_message(&text, &mentions, reply_to.map(MessageId)),
        )?
        .0)
    }

    /// Sends a picked image (the file's bytes: JPEG, PNG, GIF or WebP) to the
    /// main group with an optional caption and `mentions` (UTF-16 ranges of
    /// the caption); returns its local id. The core makes the WebP and queues
    /// the message at once, connected or not, like a text; it uploads the
    /// picture and sends the message when it can (message-content: Image
    /// messages; client-runtime: Blob crypto and send order).
    pub async fn send_image(
        &self,
        picked: Vec<u8>,
        caption: Option<String>,
        mentions: Vec<MentionSpan>,
    ) -> Result<i64, CoreError> {
        self.send_image_in(0, picked, caption, mentions).await
    }

    /// [`Core::send_image`] in chat `chat_id`.
    pub async fn send_image_in(
        &self,
        chat_id: i64,
        picked: Vec<u8>,
        caption: Option<String>,
        mentions: Vec<MentionSpan>,
    ) -> Result<i64, CoreError> {
        let mentions = map::mentions(caption.as_deref().unwrap_or_default(), &mentions)?;
        Ok(forward(
            self.client
                .send_image_in(ChatId(chat_id), &picked, caption.as_deref(), &mentions)
                .await,
        )?
        .0)
    }

    /// Sends a subject cutout as a sticker to chat `chat_id` (photo-sticker:
    /// Sticker is an image; Sticker styling and encode). `rgba` is the
    /// straight-alpha RGBA the platform's `SegmentationSource` produced
    /// (R,G,B,A byte order, row stride `width`×4, orientation applied,
    /// `width`×`height`×4 bytes). The core trims it, encodes WebP preserving the
    /// alpha and sends it down the unchanged image blob path; the native side
    /// encodes nothing and never sees the key. Queued at once like an image,
    /// connected or not. A sticker has no caption. Returns its local id.
    pub async fn send_sticker_in(
        &self,
        chat_id: i64,
        source: StickerSource,
        rgba: Vec<u8>,
        width: u32,
        height: u32,
    ) -> Result<i64, CoreError> {
        let cutout = client::media::RgbaCutout {
            source: source.to_core(),
            width,
            height,
            pixels: rgba,
        };
        Ok(forward(self.client.send_sticker_in(ChatId(chat_id), cutout).await)?.0)
    }

    /// The picture of image message `id` as WebP bytes: from this device's
    /// cache, else downloaded, decrypted and cached (client-runtime: Image
    /// download and cache). Fails with [`CoreErrorKind::MediaExpired`] when
    /// the server no longer has it.
    pub async fn download_image(&self, id: i64) -> Result<Vec<u8>, CoreError> {
        forward(self.client.download_image(MessageId(id)).await)
    }

    /// The picture of image message `id` if this device has it cached; never
    /// uses the network.
    pub async fn cached_image(&self, id: i64) -> Result<Option<Vec<u8>>, CoreError> {
        forward(self.client.cached_image(MessageId(id)).await)
    }

    /// Resends the picture of the own image message `id` as a new message
    /// with a new blob, when the first one expired (client-runtime: Expired
    /// media); returns the new message's local id.
    pub async fn resend_image(&self, id: i64) -> Result<i64, CoreError> {
        Ok(forward(self.client.resend_image(MessageId(id)).await)?.0)
    }

    /// Sends a recorded voice message to the main group; returns its local id.
    /// `wav` is the file the recorder wrote (16-bit PCM). The core makes the
    /// OGG/Opus mono clip and its waveform, refuses one over two minutes with
    /// [`CoreErrorKind::AudioTooLong`], and queues the message at once,
    /// connected or not, like an image (audio-messages: Recording and encoding).
    /// When `reply_to` is set, the voice message answers that message
    /// (message-content: Voice replies).
    pub async fn send_audio(&self, wav: Vec<u8>, reply_to: Option<i64>) -> Result<i64, CoreError> {
        self.send_audio_in(0, wav, reply_to).await
    }

    /// [`Core::send_audio`] in chat `chat_id`.
    pub async fn send_audio_in(
        &self,
        chat_id: i64,
        wav: Vec<u8>,
        reply_to: Option<i64>,
    ) -> Result<i64, CoreError> {
        Ok(forward(
            self.client
                .send_audio_in(ChatId(chat_id), &wav, reply_to.map(MessageId))
                .await,
        )?
        .0)
    }

    /// The path of voice message `id`'s clip, an OGG/Opus file in the core's
    /// media cache for a player to open: from the cache, else downloaded,
    /// decrypted and cached (client-runtime: Image download and cache). Dart
    /// never copies or keeps the file. Fails with
    /// [`CoreErrorKind::MediaExpired`] when the server no longer has it.
    pub async fn download_audio(&self, id: i64) -> Result<String, CoreError> {
        Ok(path_text(forward(
            self.client.download_audio(MessageId(id)).await,
        )?))
    }

    /// The path of voice message `id`'s clip if this device has it cached;
    /// never uses the network.
    pub async fn cached_audio(&self, id: i64) -> Result<Option<String>, CoreError> {
        Ok(forward(self.client.cached_audio(MessageId(id)).await)?.map(path_text))
    }

    /// Resends the clip of the own voice message `id` as a new message with a
    /// new blob, when the first one expired (client-runtime: Expired media);
    /// returns the new message's local id.
    pub async fn resend_audio(&self, id: i64) -> Result<i64, CoreError> {
        Ok(forward(self.client.resend_audio(MessageId(id)).await)?.0)
    }

    /// Sends a video to the main group; returns its local id.
    /// [`Core::send_video_in`] in the main group.
    #[allow(clippy::too_many_arguments)]
    pub async fn send_video(
        &self,
        mp4: Vec<u8>,
        duration_ms: u32,
        width: u32,
        height: u32,
        first_frame_rgba: Vec<u8>,
        frame_width: u32,
        frame_height: u32,
    ) -> Result<i64, CoreError> {
        self.send_video_in(
            0,
            mp4,
            duration_ms,
            width,
            height,
            first_frame_rgba,
            frame_width,
            frame_height,
        )
        .await
    }

    /// Sends a video to chat `chat_id`; returns its local id. `mp4` is the
    /// already-normalized clip the app's native plugin produced (H.264/MP4 at the
    /// 480p/~1 Mbps target, ≤ 30 s); the core does only the blob crypto
    /// (video-messages: Video normalization in a native plugin).
    /// `first_frame_rgba` (RGBA, `frame_width` x `frame_height`) is the first
    /// frame the core turns into the body's ThumbHash — pass an empty list for no
    /// blur. The core refuses a clip over 30 s with
    /// [`CoreErrorKind::VideoTooLong`], one whose encrypted blob exceeds the
    /// server's advertised cap with [`CoreErrorKind::VideoTooLarge`], and
    /// non-MP4 bytes with [`CoreErrorKind::NotAVideo`]; otherwise it queues the
    /// message at once, connected or not, and uploads then sends it like an image.
    #[allow(clippy::too_many_arguments)]
    pub async fn send_video_in(
        &self,
        chat_id: i64,
        mp4: Vec<u8>,
        duration_ms: u32,
        width: u32,
        height: u32,
        first_frame_rgba: Vec<u8>,
        frame_width: u32,
        frame_height: u32,
    ) -> Result<i64, CoreError> {
        Ok(forward(
            self.client
                .send_video_in(
                    ChatId(chat_id),
                    &mp4,
                    duration_ms,
                    width,
                    height,
                    &first_frame_rgba,
                    frame_width,
                    frame_height,
                )
                .await,
        )?
        .0)
    }

    /// The path of video message `id`'s clip, an MP4 file in the core's media
    /// cache for a player to open: from the cache, else downloaded, decrypted and
    /// cached (client-runtime: Image download and cache). Dart never copies or
    /// keeps the file. Fails with [`CoreErrorKind::MediaExpired`] when the server
    /// no longer has it.
    pub async fn download_video(&self, id: i64) -> Result<String, CoreError> {
        Ok(path_text(forward(
            self.client.download_video(MessageId(id)).await,
        )?))
    }

    /// The path of video message `id`'s clip if this device has it cached; never
    /// uses the network.
    pub async fn cached_video(&self, id: i64) -> Result<Option<String>, CoreError> {
        Ok(forward(self.client.cached_video(MessageId(id)).await)?.map(path_text))
    }

    /// Resends the clip of the own video message `id` as a new message with a new
    /// blob, when the first one expired (client-runtime: Expired media); returns
    /// the new message's local id.
    pub async fn resend_video(&self, id: i64) -> Result<i64, CoreError> {
        Ok(forward(self.client.resend_video(MessageId(id)).await)?.0)
    }

    /// The local ids of messages whose mention of this user is not seen yet,
    /// in chat order (client-runtime: Unseen mentions).
    pub fn unseen_mentions(&self) -> Result<Vec<i64>, CoreError> {
        Ok(forward(self.client.unseen_mentions())?
            .into_iter()
            .map(|id| id.0)
            .collect())
    }

    /// Marks the mention in message `id` seen; `false` if none was unseen.
    pub fn mark_mention_seen(&self, id: i64) -> Result<bool, CoreError> {
        forward(self.client.mark_mention_seen(MessageId(id)))
    }

    /// Sets this user's display name and "about" text for every member.
    pub fn set_profile(
        &self,
        display_name: Option<String>,
        about: Option<String>,
    ) -> Result<i64, CoreError> {
        Ok(forward(
            self.client
                .set_profile(display_name.as_deref(), about.as_deref()),
        )?
        .0)
    }

    /// The newest `limit` messages, oldest first.
    pub fn history(&self, limit: u32) -> Result<Vec<ChatMessage>, CoreError> {
        Ok(forward(self.client.history(limit as usize))?
            .into_iter()
            .map(map::message)
            .collect())
    }

    /// Whether any own message is still sending (client-runtime: Unsent messages
    /// query). The app asks it when it goes to the background.
    pub fn has_unsent(&self) -> Result<bool, CoreError> {
        forward(self.client.has_unsent())
    }

    /// One message by local id.
    pub fn message(&self, id: i64) -> Result<Option<ChatMessage>, CoreError> {
        Ok(forward(self.client.message(MessageId(id)))?.map(map::message))
    }

    /// `user`'s profile.
    pub fn profile(&self, user: UserRef) -> Result<ProfileInfo, CoreError> {
        let profile = forward(self.client.profile(&map::user_id(&user)))?;
        Ok(ProfileInfo {
            display_name: profile.display_name,
            about: profile.about,
        })
    }

    /// How to show `user`: "Ali (alice)", or the username alone.
    pub fn display_name(&self, user: UserRef) -> Result<String, CoreError> {
        forward(self.client.display_name(&map::user_id(&user)))
    }

    /// The group's member devices.
    pub fn members(&self) -> Result<Vec<MemberDevice>, CoreError> {
        Ok(forward(self.client.members())?
            .iter()
            .map(map::member)
            .collect())
    }

    /// The admin's user, as the group names it (visible to every member);
    /// unset before this device is in the group.
    pub fn admin_user(&self) -> Result<Option<UserRef>, CoreError> {
        Ok(forward(self.client.admin_user())?.as_ref().map(map::user))
    }

    /// The group's epoch on this device, once in the group.
    pub fn epoch(&self) -> Result<Option<u64>, CoreError> {
        forward(self.client.epoch())
    }

    // --- Chats (direct-messages; app-ui: Chat list, change `dms`) ---

    /// Whether the server knows DMs: the app offers "Message" only then.
    pub fn dms_available(&self) -> bool {
        self.client.dms_available()
    }

    /// Whether the server knows named groups: the app offers "Create group"
    /// only then.
    pub fn named_groups_available(&self) -> bool {
        self.client.named_groups_available()
    }

    /// Admin: creates a named group with `name` and adds `members`; its chat
    /// id. The adds finish in the background (a chats-changed event follows).
    pub async fn create_named_group(
        &self,
        name: Option<String>,
        members: Vec<UserRef>,
    ) -> Result<i64, CoreError> {
        let members: Vec<_> = members.iter().map(map::user_id).collect();
        Ok(forward(
            self.client
                .create_named_group(name.as_deref(), &members)
                .await,
        )?
        .0)
    }

    /// Admin: adds all of `user`'s devices to the named group `chat_id`.
    pub async fn add_to_group(&self, chat_id: i64, user: UserRef) -> Result<(), CoreError> {
        forward(
            self.client
                .add_to_group(ChatId(chat_id), &map::user_id(&user))
                .await,
        )
    }

    /// Admin: removes `user` from the named group `chat_id` (not from the lobby).
    pub async fn remove_from_group(&self, chat_id: i64, user: UserRef) -> Result<(), CoreError> {
        forward(
            self.client
                .remove_from_group(ChatId(chat_id), &map::user_id(&user))
                .await,
        )
    }

    /// Admin: sets the name of the named group `chat_id` for every member.
    pub fn rename_group(&self, chat_id: i64, name: String) -> Result<i64, CoreError> {
        Ok(forward(self.client.rename_group(ChatId(chat_id), &name))?.0)
    }

    /// Admin: closes the named group `chat_id`; it becomes read-only.
    pub async fn close_group(&self, chat_id: i64) -> Result<(), CoreError> {
        forward(self.client.close_group(ChatId(chat_id)).await)
    }

    /// Removes the closed DM `chat_id` (its peer was removed from the server):
    /// tells the server, then deletes the chat and its messages here; the
    /// user's other devices follow on their next connect. `NoSuchChat` unless it
    /// is a closed DM; nothing is deleted if the server call fails.
    pub async fn remove_closed_dm(&self, chat_id: i64) -> Result<(), CoreError> {
        forward(self.client.remove_closed_dm(ChatId(chat_id)).await)
    }

    /// Mutes or unmutes the conversation `chat_id` (notification-prefs: Mute a
    /// conversation): tells the server, then records it here and on the user's
    /// other devices. A muted conversation raises no wake push and no in-app
    /// notification while its messages still arrive. Fails (leaving it unmuted)
    /// if the server does not support it or is unreachable.
    pub async fn set_conversation_muted(&self, chat_id: i64, muted: bool) -> Result<(), CoreError> {
        forward(
            self.client
                .set_mute_pref(MuteTarget::Conversation(ChatId(chat_id)), muted)
                .await,
        )
    }

    /// Turns whole-app mute on or off (notification-prefs: Mute the whole app):
    /// while on, every conversation is silenced, overriding the per-conversation
    /// mutes.
    pub async fn set_app_muted(&self, muted: bool) -> Result<(), CoreError> {
        forward(self.client.set_mute_pref(MuteTarget::Global, muted).await)
    }

    /// Whether whole-app mute is on (notification-prefs: Mute the whole app).
    pub fn app_muted(&self) -> Result<bool, CoreError> {
        Ok(forward(self.client.mute_prefs())?.muted_all)
    }

    /// manual-presence: Setting your status. Sets this user's own manual flag —
    /// `true` for Online, `false` for Offline — recording it on this device and
    /// sending it to the server (now if connected, else on the next connect). The
    /// server distributes it to everyone who shares a conversation with this user.
    pub fn set_presence(&self, online: bool) -> Result<(), CoreError> {
        let status = if online {
            Presence::Online
        } else {
            Presence::Offline
        };
        forward(self.client.set_presence(status))
    }

    /// This user's own manual presence flag (manual-presence: Manual status,
    /// never inferred).
    pub fn my_presence(&self) -> Result<PresenceStatus, CoreError> {
        Ok(map::presence(forward(self.client.my_presence())?))
    }

    /// The manual presence flag this device last learned for `user`, or `Unknown`
    /// when it has learned none — the app then shows no dot (manual-presence:
    /// Presence indicator on avatars).
    pub fn presence_of(&self, user: UserRef) -> Result<PresenceStatus, CoreError> {
        let user = UserId::new(user.username, user.server_id);
        Ok(map::presence_opt(forward(self.client.presence_of(&user))?))
    }

    /// The member devices of the named group `chat_id`.
    pub fn group_members(&self, chat_id: i64) -> Result<Vec<MemberDevice>, CoreError> {
        Ok(forward(self.client.group_members(ChatId(chat_id)))?
            .iter()
            .map(map::member)
            .collect())
    }

    /// The chats: the lobby first, then named groups and DMs, newest first,
    /// closed last.
    pub fn chats(&self) -> Result<Vec<ChatSummary>, CoreError> {
        Ok(forward(self.client.chats())?
            .into_iter()
            .map(map::chat)
            .collect())
    }

    /// The open DM with `user`, opened first if there is none; its chat id.
    pub async fn open_dm(&self, user: UserRef) -> Result<i64, CoreError> {
        Ok(forward(self.client.open_dm(&map::user_id(&user)).await)?.0)
    }

    /// The newest `limit` messages of chat `chat_id`, oldest first.
    pub fn history_in(&self, chat_id: i64, limit: u32) -> Result<Vec<ChatMessage>, CoreError> {
        Ok(
            forward(self.client.history_in(ChatId(chat_id), limit as usize))?
                .into_iter()
                .map(map::message)
                .collect(),
        )
    }

    /// [`Core::send_message`] in chat `chat_id`.
    pub fn send_message_in(
        &self,
        chat_id: i64,
        text: String,
        mentions: Vec<MentionSpan>,
        reply_to: Option<i64>,
    ) -> Result<i64, CoreError> {
        let mentions = map::mentions(&text, &mentions)?;
        Ok(forward(self.client.send_message_in(
            ChatId(chat_id),
            &text,
            &mentions,
            reply_to.map(MessageId),
        ))?
        .0)
    }

    /// message-deletion: Who may delete. Deletes the stored chat message `id`
    /// in chat `chat_id` for everyone: an admin may delete any message in the
    /// lobby or a named group, and in a DM a user may delete their own. Returns
    /// the local id of the queued delete.
    pub fn delete_message(&self, chat_id: i64, id: i64) -> Result<i64, CoreError> {
        Ok(forward(self.client.delete_message(ChatId(chat_id), MessageId(id)))?.0)
    }

    /// message-deletion: Receiver may reveal in a DM. The original of the deleted
    /// DM message `id` in chat `chat_id`, if this device still holds it.
    pub fn reveal(&self, chat_id: i64, id: i64) -> Result<ChatMessage, CoreError> {
        Ok(map::message(forward(
            self.client.reveal(ChatId(chat_id), MessageId(id)),
        )?))
    }

    /// [`Core::unseen_mentions`] in chat `chat_id`.
    pub fn unseen_mentions_in(&self, chat_id: i64) -> Result<Vec<i64>, CoreError> {
        Ok(forward(self.client.unseen_mentions_in(ChatId(chat_id)))?
            .into_iter()
            .map(|id| id.0)
            .collect())
    }

    /// The user has seen chat `chat_id` up to its newest message.
    pub fn mark_chat_read(&self, chat_id: i64) -> Result<(), CoreError> {
        forward(self.client.mark_chat_read(ChatId(chat_id)))
    }

    /// The read mark of chat `chat_id`: the local id of the newest message
    /// read last, or 0 when it was never read. The app reads it when a chat
    /// opens to place the "New messages" divider (app-ui: Unread divider).
    pub fn chat_read_mark(&self, chat_id: i64) -> Result<i64, CoreError> {
        Ok(forward(self.client.read_mark(ChatId(chat_id)))?.0)
    }
}
