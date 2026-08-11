//! Tauri commands — the IPC surface. Keep thin: parse args, delegate to
//! a focused helper, return serde-ready data.

mod comparisons;
mod experiments;
mod jobs;
mod pages;
mod papers;
mod runtime_info;
mod settings;
mod srs;

pub use comparisons::*;
pub use experiments::*;
pub use jobs::*;
pub use pages::*;
pub use papers::*;
pub use runtime_info::*;
pub use settings::*;
pub use srs::*;

use serde::Serialize;

/// Common error surface for commands — maps anyhow / sqlx / io into a plain
/// string the frontend can render.
#[derive(Debug, Serialize)]
pub struct CmdError(pub String);

impl<E: std::fmt::Display> From<E> for CmdError {
    fn from(e: E) -> Self {
        Self(format!("{e:#}"))
    }
}

pub type CmdResult<T> = std::result::Result<T, CmdError>;
