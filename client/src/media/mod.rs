//! Images and voice messages in the core (changes `media-blobs`, `audio-messages`):
//! the encode pipelines, blob crypto and the device media cache. The transfer to
//! and from the server lives in [`crate::Client`], which holds the session.
//!
//! Only the core touches image and audio bytes: Dart passes the picked file or
//! the recorded WAV in and gets WebP bytes (or the path of an OGG file) back to
//! show or play, and never transforms, stores or sends them.

pub mod audio;
pub mod cache;
pub mod crypto;
pub mod encode;
pub mod video;

pub use audio::{AudioError, EncodedAudio};
pub use cache::{DEFAULT_CACHE_BYTES, MediaCache, MediaKind};
pub use crypto::{BlobId, BlobKey, CryptoError};
pub use encode::{EncodedImage, ImageError};

use crate::store::StoreError;

/// Why an image operation failed below the [`crate::Client`] API.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum MediaError {
    /// The picked image cannot be sent.
    #[error(transparent)]
    Image(#[from] ImageError),
    /// The recorded voice message cannot be sent.
    #[error(transparent)]
    Audio(#[from] AudioError),
    /// A blob could not be made or opened.
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    /// The media directory could not be read or written.
    #[error("media cache: {0}")]
    Io(#[from] std::io::Error),
    /// The cache index in the local store failed.
    #[error(transparent)]
    Store(#[from] StoreError),
}
