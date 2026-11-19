//! Images in the core (change `media-blobs`): the encode pipeline, blob
//! crypto and the device media cache. The transfer to and from the Pi lives in
//! [`crate::Client`], which holds the session.
//!
//! Only the core touches image bytes: Dart passes the picked file in and gets
//! WebP bytes back to draw, and never transforms, stores or sends them.

pub mod cache;
pub mod crypto;
pub mod encode;

pub use cache::{DEFAULT_CACHE_BYTES, MediaCache};
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
