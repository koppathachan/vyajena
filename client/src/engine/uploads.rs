//! Uploading the blob of a queued image or voice message at send time
//! (client-runtime: Blob crypto and send order).
//!
//! An own image message waits in the outbox like text, whether or not the
//! device is connected; its plaintext WebP is in the media cache and its key and
//! blob id are in its stored `Content`. When the outbox reaches it with a
//! connection, it uploads the blob first and sends the envelope only after the
//! Server accepted the blob, so a receiver never sees an `Image` whose blob is
//! missing, however long the message waited.
//!
//! The upload runs in a task of its own, so a slow link does not stall the
//! heartbeats and the stream; the task reports back as
//! [`Command::Uploaded`]. The chat's later messages wait behind the image, as
//! they do behind any message that cannot go out yet.

use std::sync::Arc;
use std::time::{Instant, SystemTime};

use protocol::blob_store::BLOB_TTL;

use super::{Command, Engine, EngineError, UPLOAD_RETRY_DELAY};
use crate::events::MessageId;
use crate::media::{BlobId, BlobKey};
use crate::media::{MediaCache, crypto};
use crate::messaging::{Decoded, decode};
use crate::net::Session;
use crate::pb;
use crate::store::{Store, StoredMessage};

/// The most consecutive `UNAVAILABLE` answers from `UploadBlob` tolerated before
/// an upload fails. At [`UPLOAD_RETRY_DELAY`] (3s) each, this is ~30s: long enough
/// to ride out a drive remount, short enough that a server with no blob store
/// surfaces an error instead of a silent 7-day (blob-TTL) hourglass
/// (client-runtime: Media the server will not store fails promptly).
pub(super) const MAX_BLOB_UNAVAILABLE_RETRIES: u32 = 10;

/// Where one queued image's upload stands.
pub(super) enum Upload {
    /// A task is uploading it. `unavailable` is the count of consecutive
    /// `UNAVAILABLE` answers that led here, carried across retries.
    Running { unavailable: u32 },
    /// It failed in a way that may pass: try again then. `unavailable` is the
    /// running count of consecutive `UNAVAILABLE` answers.
    RetryAt { at: Instant, unavailable: u32 },
}

/// How an upload ended.
#[derive(Debug)]
pub(crate) enum Outcome {
    /// The server has the blob.
    Done,
    /// It did not work now (no connection, the session ran out, a timeout): try
    /// again.
    Retry(String),
    /// The server answered `UNAVAILABLE` (its blob store is off or its drive is
    /// away). Retry a bounded number of times, then fail; the string is the noun
    /// ("picture" / "voice message") for the failure reason.
    Unavailable(String),
    /// It will never work: the message fails with this reason.
    Fail(String),
}

/// What a queued blob holds, for the messages that name it.
#[derive(Debug, Clone, Copy)]
enum Noun {
    Picture,
    VoiceMessage,
    Video,
}

impl Noun {
    fn text(self) -> &'static str {
        match self {
            Self::Picture => "picture",
            Self::VoiceMessage => "voice message",
            Self::Video => "video",
        }
    }
}

/// Whether a status from `UploadBlob` is one a retry can fix.
fn classify(status: &tonic::Status, noun: Noun) -> Outcome {
    use tonic::Code::{AlreadyExists, InvalidArgument, NotFound, PermissionDenied, Unavailable};
    match status.code() {
        // Not a member of the group, a blob the server will not take: no retry
        // changes that.
        PermissionDenied | InvalidArgument | AlreadyExists | NotFound => Outcome::Fail(format!(
            "the server refused the {} ({})",
            noun.text(),
            status.message()
        )),
        // Unavailable: the server's blob store is off (no `[blob_store]`) or its
        // drive is away. Retry a bounded number of times, then fail, so a server
        // that stores no media surfaces an error instead of hanging forever.
        Unavailable => Outcome::Unavailable(noun.text().to_owned()),
        // Unauthenticated (the session ran out), a timeout: wait and retry.
        code => Outcome::Retry(format!("{code}: {}", status.message())),
    }
}

/// Encrypts the cached picture or clip and uploads it.
async fn upload(
    store: Arc<Store>,
    cache: MediaCache,
    session: Session,
    group_id: Vec<u8>,
    (blob_id, key, noun): (BlobId, BlobKey, Noun),
    now: SystemTime,
) -> Outcome {
    let sealed = tokio::task::spawn_blocking(move || {
        let path = cache
            .get_path(&store, &blob_id, now)
            .map_err(|error| Outcome::Retry(error.to_string()))?
            .ok_or_else(|| {
                Outcome::Fail(format!("the {} is no longer on this device", noun.text()))
            })?;
        let plain = std::fs::read(path).map_err(|error| Outcome::Retry(error.to_string()))?;
        crypto::encrypt(&key, &blob_id, &plain).map_err(|error| Outcome::Fail(error.to_string()))
    })
    .await;
    let ciphertext = match sealed {
        Ok(Ok(ciphertext)) => ciphertext,
        Ok(Err(outcome)) => return outcome,
        Err(error) => return Outcome::Retry(error.to_string()),
    };
    let request = match session.request(pb::UploadBlobRequest {
        group_id,
        blob_id: blob_id.to_vec(),
        ciphertext,
    }) {
        Ok(request) => request,
        Err(error) => return Outcome::Retry(error.to_string()),
    };
    match session.conn().blobs().upload_blob(request).await {
        Ok(_) => Outcome::Done,
        Err(status) => classify(&status, noun),
    }
}

impl Engine {
    /// Starts uploading the blob of the queued image or voice `message`, whose chat is
    /// `group_id`, unless an upload runs or waits to retry. A message waiting
    /// longer than the blob would live fails instead.
    pub(super) fn start_upload(
        &mut self,
        message: &StoredMessage,
        group_id: Vec<u8>,
    ) -> Result<(), EngineError> {
        // Carry the consecutive-UNAVAILABLE count across a retry so the bound
        // in `on_uploaded` counts real server answers, not restarts.
        let unavailable = match self.uploads.get(&message.id) {
            Some(Upload::Running { .. }) => return Ok(()),
            Some(Upload::RetryAt { at, .. }) if Instant::now() < *at => return Ok(()),
            Some(Upload::RetryAt { unavailable, .. }) => *unavailable,
            None => 0,
        };
        let blob = match decode(&message.content).body {
            Decoded::Image(image) => (image.blob_id, image.blob_key, Noun::Picture),
            Decoded::Audio(audio) => (audio.blob_id, audio.blob_key, Noun::VoiceMessage),
            Decoded::Video(video) => (video.blob_id, video.blob_key, Noun::Video),
            _ => {
                // Marked pending but carrying no blob: nothing to upload.
                self.store().set_blob_pending(message.id, false)?;
                return Ok(());
            }
        };
        let waited = self
            .now()
            .duration_since(message.created_at)
            .unwrap_or_default();
        if waited >= BLOB_TTL {
            self.uploads.remove(&message.id);
            return self.fail(
                message.id,
                &format!("the {} could not be sent in time", blob.2.text()),
            );
        }
        let Some(link) = self.link.as_ref() else {
            return Ok(());
        };
        let session = link.session.clone();
        self.uploads
            .insert(message.id, Upload::Running { unavailable });
        let (id, now) = (message.id, self.now());
        let (tx, store) = (self.command_tx.clone(), Arc::clone(&self.shared.store));
        let cache = self.shared.media.clone();
        tokio::spawn(async move {
            let outcome = upload(store, cache, session, group_id, blob, now).await;
            let _ = tx.send(Command::Uploaded { id, outcome }).await;
        });
        Ok(())
    }

    /// An upload ended: the message goes out, waits to try again, or fails.
    pub(super) async fn on_uploaded(
        &mut self,
        id: MessageId,
        outcome: Outcome,
    ) -> Result<(), EngineError> {
        // The count of consecutive UNAVAILABLE answers so far (the upload was
        // Running with it carried over from the previous retry).
        let unavailable = match self.uploads.get(&id) {
            Some(Upload::Running { unavailable }) => *unavailable,
            _ => 0,
        };
        match outcome {
            Outcome::Done => {
                self.uploads.remove(&id);
                self.store().set_blob_pending(id, false)?;
                tracing::debug!(id = id.0, "uploaded the blob of a queued image");
            }
            Outcome::Retry(why) => {
                tracing::debug!(id = id.0, %why, "an image upload will be tried again");
                // A non-UNAVAILABLE retry (auth, timeout) resets the count.
                self.uploads.insert(
                    id,
                    Upload::RetryAt {
                        at: Instant::now() + UPLOAD_RETRY_DELAY,
                        unavailable: 0,
                    },
                );
            }
            Outcome::Unavailable(noun) => {
                let count = unavailable + 1;
                if count >= MAX_BLOB_UNAVAILABLE_RETRIES {
                    let reason = format!("the server is not storing {noun}s right now");
                    tracing::warn!(id = id.0, %reason, "a blob upload gave up after UNAVAILABLE");
                    self.uploads.remove(&id);
                    self.fail(id, &reason)?;
                } else {
                    tracing::debug!(
                        id = id.0,
                        count,
                        "the server is UNAVAILABLE for this blob; will try again"
                    );
                    self.uploads.insert(
                        id,
                        Upload::RetryAt {
                            at: Instant::now() + UPLOAD_RETRY_DELAY,
                            unavailable: count,
                        },
                    );
                }
            }
            Outcome::Fail(reason) => {
                tracing::warn!(id = id.0, %reason, "an image upload failed for good");
                self.uploads.remove(&id);
                self.fail(id, &reason)?;
            }
        }
        self.flush_outbox().await
    }

    /// Whether an upload that waited to retry is due.
    pub(super) fn upload_due(&self) -> bool {
        self.uploads
            .values()
            .any(|upload| matches!(upload, Upload::RetryAt { at, .. } if Instant::now() >= *at))
    }

    /// A new connection: retries that waited go at once.
    pub(super) fn retry_uploads_now(&mut self) {
        self.uploads
            .retain(|_, upload| matches!(upload, Upload::Running { .. }));
    }
}
