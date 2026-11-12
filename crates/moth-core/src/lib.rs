//! UI-independent Moth domain model. No Tauri, OS calls or network access.
mod import;
mod model;
pub use import::{import_links, ImportError};
pub use model::*;
