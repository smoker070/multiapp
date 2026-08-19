//! Multiapp engine — one implementation for macOS, Windows and Linux.
//!
//! This exists because maintaining a second hand-written implementation per OS produced bugs that
//! could not be tested away: a PowerShell port that swept the Windows credential vault into a backup
//! archive, and a shell profile-matcher that reported stopped profiles as running.
pub mod error;
pub mod launch;
pub mod paths;
pub mod proc;
pub mod profile;

pub use error::Error;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
