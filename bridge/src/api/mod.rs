//! Everything Dart sees (flutter-app design D2, D5).
//!
//! - [`core`]: [`core::Core`], one per app process, wrapping
//!   [`client::Client`], with the one event stream.
//! - [`types`]: the plain types it takes and returns.
//! - [`error`]: [`error::CoreError`], what every failing call throws in Dart.
//! - [`text`]: the text forms of invites, link requests and link approvals.

#![forbid(unsafe_code)]

pub mod core;
pub mod error;
pub mod text;
pub mod types;
