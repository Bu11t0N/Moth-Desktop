use serde::{Deserialize, Serialize};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LicenseKind {
    Project,
    Rust,
    JavaScript,
    ThirdParty,
}
#[derive(Serialize)]
pub struct Document {
    name: &'static str,
    mime: &'static str,
    text: &'static str,
}
#[tauri::command]
pub fn license_document(kind: LicenseKind) -> Document {
    match kind {
        LicenseKind::Project => Document {
            name: "Moth-GPL-3.0.txt",
            mime: "text/plain",
            text: include_str!("../../LICENSE"),
        },
        LicenseKind::Rust => Document {
            name: "Moth-Rust-dependencies.html",
            mime: "text/html",
            text: include_str!("../../licenses/Rust-dependencies.html"),
        },
        LicenseKind::JavaScript => Document {
            name: "Moth-Tauri-API-MIT.txt",
            mime: "text/plain",
            text: include_str!("../../licenses/tauri-api-MIT.txt"),
        },
        LicenseKind::ThirdParty => Document {
            name: "Moth-third-party.txt",
            mime: "text/plain",
            text: include_str!("../../THIRD_PARTY.md"),
        },
    }
}
