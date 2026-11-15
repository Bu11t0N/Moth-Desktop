# Third-party notices

This project is related to [Moth Android](https://github.com/Bu11t0N/Moth), GPL-3.0-or-later.
Android behavior and backup formats are references; this preview implements new Rust/TS code.

Direct dependencies include Tauri (MIT/Apache-2.0), Serde/serde_json (MIT/Apache-2.0),
uuid (MIT/Apache-2.0), url (MIT/Apache-2.0), base64 (MIT/Apache-2.0), percent-encoding
(MIT/Apache-2.0), thiserror (MIT/Apache-2.0), windows-sys (MIT/Apache-2.0), zeroize
(MIT/Apache-2.0), tempfile (MIT/Apache-2.0), TypeScript (Apache-2.0), Vite (MIT)
and Playwright (Apache-2.0), Prettier (MIT). Cargo.lock and package-lock.json pin resolved dependencies.

Development packages now include unmodified official Xray 26.10.21 (MPL-2.0) and
sing-box 1.12.25 (GPL-3.0-or-later) executables. Versions, exact archive/executable
hashes and source locations are in `engines.lock.json`; full source revisions and
build references are in [docs/ENGINES.md](docs/ENGINES.md). Source is publicly available
at those exact revisions. No Android patches are applied to these executables.
Xray's license is in `licenses/Xray-MPL-2.0.txt`; sing-box's copyright/license notice
is in `licenses/sing-box-GPL-3.0.txt` and the full GPLv3 text is in `LICENSE`.
The official sing-box executable embeds Wintun through sing-tun v0.7.13; no independent DLL is shipped. Its source includes the WireGuard MIT wrapper and the upstream embedded binary. See docs/ARCHITECTURE.md for the exact source. Moth is not an
official SagerNet or XTLS client. Retain all these notices alongside the executables.

Additional Rust dependencies include Tokio, reqwest and sha2 (MIT/Apache-2.0);
windows-service and widestring (MIT/Apache-2.0) support the Windows service.
resolved dependencies are pinned in Cargo.lock. The engine source trees include their
go.mod/go.sum and upstream build configuration for the transitive Go dependency graph.
The complete locked Rust runtime graph for Windows x64 and Linux x64 is included
in `licenses/Rust-dependencies.html`, generated with cargo-about 0.9.3, `about.toml`
and `packaging/licenses.hbs`. The bundled JavaScript runtime is @tauri-apps/api;
its notice is in `licenses/tauri-api-MIT.txt`. `licenses/dependency-inventory.json`
records source-lock/configuration and output hashes; `python scripts/license-notices.py`
checks them. To regenerate after dependency changes, run `npm ci`, then
`python scripts/license-notices.py generate PATH_TO_CARGO_ABOUT_0_9_3`.
Build-only dependencies are excluded. The upstream engine Go dependency notices
still require a separate complete inventory before production publication.

The embedded amd64 Wintun DLL matches official Wintun 0.14.1: SHA-256 e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce. Official archive SHA-256: 07c256185d6ee3652e09fa55c0b673e2624b565e02c4b9091c79ca7d2f24ef51 (https://www.wintun.net/builds/wintun-0.14.1.zip). The prebuilt-binary license is preserved in licenses/Wintun-0.14.1.txt. Git blob aee04e77bfbd6601adbcd5a46435f059e31cf6e4 matches sing-tun v0.7.13 internal/wintun/amd64/wintun.dll.

Version 0.3.2 enables Tauri tray-icon support and adds tauri-plugin-single-instance 2.5.5 (https://github.com/tauri-apps/plugins-workspace) and tray-icon 0.25.1 (https://github.com/tauri-apps/tray-icon). Both are MIT OR Apache-2.0; their license texts are preserved in licenses/single-instance-* and licenses/tray-icon-*. Cargo.lock pins their resolved versions.
