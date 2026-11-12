//! UI-independent Moth domain model. No Tauri, OS calls or network access.
mod import;
mod model;
mod share;
mod tunnel;
pub use import::{import_links, ImportError};
pub use model::*;
pub use share::share_link;
pub use tunnel::{sing_box_tun_config, xray_bridge_config, LocalBridge};
