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
No Wintun DLL or independent driver is bundled in this milestone. Moth is not an
official SagerNet or XTLS client. Retain all these notices alongside the executables.

Additional Rust dependencies include Tokio, reqwest and sha2 (MIT/Apache-2.0);
windows-service and widestring (MIT/Apache-2.0) support the Windows service.
resolved dependencies are pinned in Cargo.lock. The engine source trees include their
go.mod/go.sum and upstream build configuration for the transitive Go dependency graph.
Dependency/license inventory is a release gate; this short list is not a complete transitive inventory.
