//! Uploading the blob of a queued image message at send time (client-runtime:
//! Blob crypto and send order).
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
use crate::media::{MediaCache, crypto};
use crate::messaging::{Decoded, ImageContent, decode};
use crate::net::Session;
use crate::pb;
use crate::store::{Store, StoredMessage};

/// Where one queued image's upload stands.
pub(super) enum Upload {
    /// A task is uploading it.
    Running,
    /// It failed in a way that may pass: try again then.
    RetryAt(Instant),
}

/// How an upload ended.
#[derive(Debug)]
pub(crate) enum Outcome {
    /// The server has the blob.
    Done,
    /// It did not work now (no connection, the server's blob store is away, the
    /// session ran out): try again.
    Retry(String),
    /// It will never work: the message fails with this reason.
    Fail(String),
}

/// Whether a status from `UploadBlob` is one a retry can fix.
fn classify(status: &tonic::Status) -> Outcome {
    use tonic::Code::{AlreadyExists, InvalidArgument, NotFound, PermissionDenied};
    match status.code() {
        // Not a member of the group, a blob the server will not take: no retry
        // changes that.
        PermissionDenied | InvalidArgument | AlreadyExists | NotFound => Outcome::Fail(format!(
            "the server refused the picture ({})",
            status.message()
        )),
        // Unavailable (its drive is away, or it stores no pictures),
        // unauthenticated (the session ran out), a timeout: wait and retry.
        code => Outcome::Retry(format!("{code}: {}", status.message())),
    }
}

/// Encrypts the cached picture and uploads it.
async fn upload(
    store: Arc<Store>,
    cache: MediaCache,
    session: Session,
    group_id: Vec<u8>,
    image: ImageContent,
    now: SystemTime,
) -> Outcome {
    let (blob_id, key) = (image.blob_id, image.blob_key);
    let sealed = tokio::task::spawn_blocking(move || {
        let webp = cache
            .get(&store, &blob_id, now)
            .map_err(|error| Outcome::Retry(error.to_string()))?
            .ok_or_else(|| Outcome::Fail("the picture is no longer on this device".to_owned()))?;
        crypto::encrypt(&key, &blob_id, &webp).map_err(|error| Outcome::Fail(error.to_string()))
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
        Err(status) => classify(&status),
    }
}

impl Engine {
    /// Starts uploading the blob of the queued image `message`, whose chat is
    /// `group_id`, unless an upload runs or waits to retry. A message waiting
    /// longer than the blob would live fails instead.
    pub(super) fn start_upload(
        &mut self,
        message: &StoredMessage,
        group_id: Vec<u8>,
    ) -> Result<(), EngineError> {
        match self.uploads.get(&message.id) {
            Some(Upload::Running) => return Ok(()),
            Some(Upload::RetryAt(at)) if Instant::now() < *at => return Ok(()),
            _ => {}
        }
        let Decoded::Image(image) = decode(&message.content).body else {
            // Marked pending but not an image: nothing to upload.
            self.store().set_blob_pending(message.id, false)?;
            return Ok(());
        };
        let waited = self
            .now()
            .duration_since(message.created_at)
            .unwrap_or_default();
        if waited >= BLOB_TTL {
            self.uploads.remove(&message.id);
            return self.fail(message.id, "the picture could not be sent in time");
        }
        let Some(link) = self.link.as_ref() else {
            return Ok(());
        };
        let session = link.session.clone();
        self.uploads.insert(message.id, Upload::Running);
        let (id, now) = (message.id, self.now());
        let (tx, store) = (self.command_tx.clone(), Arc::clone(&self.shared.store));
        let cache = self.shared.media.clone();
        tokio::spawn(async move {
            let outcome = upload(store, cache, session, group_id, image, now).await;
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
        match outcome {
            Outcome::Done => {
                self.uploads.remove(&id);
                self.store().set_blob_pending(id, false)?;
                tracing::debug!(id = id.0, "uploaded the blob of a queued image");
            }
            Outcome::Retry(why) => {
                tracing::debug!(id = id.0, %why, "an image upload will be tried again");
                self.uploads
                    .insert(id, Upload::RetryAt(Instant::now() + UPLOAD_RETRY_DELAY));
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
            .any(|upload| matches!(upload, Upload::RetryAt(at) if Instant::now() >= *at))
    }

    /// A new connection: retries that waited go at once.
    pub(super) fn retry_uploads_now(&mut self) {
        self.uploads
            .retain(|_, upload| matches!(upload, Upload::Running));
    }
}
