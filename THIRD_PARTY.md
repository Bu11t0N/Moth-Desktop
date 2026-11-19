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
Build-only Rust/JavaScript dependencies are excluded.

`licenses/Engine-dependencies.tar.gz` preserves notices collected with
google/go-licenses v2.0.1 for both exact upstream source revisions and both target
platforms, including source files copied by that tool for copyleft dependencies.
Its SOURCE-INDEX.json also inventories all 149 distinct dependency modules read
from the four pinned binaries' Go build metadata. Each module source sum was
independently checked by `go mod download` against the binary's recorded sum;
all module license/notice/copyright/author files are included. Original Go runtime
licenses, upstream go.mod/go.sum and complete Apache/GPL texts are included.
Short upstream GPL declarations and go-nat-pmp's Apache declaration are preserved
verbatim alongside full license texts; the classifier's unknown results for these
short notices are recorded explicitly in `licenses/engine-inventory.json`.
The engine binaries are unmodified and are not rebuilt by this inventory process.
See [regeneration and exact source links](docs/DEPENDENCY-NOTICES.md).

The embedded amd64 Wintun DLL matches official Wintun 0.14.1: SHA-256 e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce. Official archive SHA-256: 07c256185d6ee3652e09fa55c0b673e2624b565e02c4b9091c79ca7d2f24ef51 (https://www.wintun.net/builds/wintun-0.14.1.zip). The prebuilt-binary license is preserved in licenses/Wintun-0.14.1.txt. Git blob aee04e77bfbd6601adbcd5a46435f059e31cf6e4 matches sing-tun v0.7.13 internal/wintun/amd64/wintun.dll.

Version 0.3.2 enables Tauri tray-icon support and adds tauri-plugin-single-instance 2.5.5 (https://github.com/tauri-apps/plugins-workspace) and tray-icon 0.25.1 (https://github.com/tauri-apps/tray-icon). Both are MIT OR Apache-2.0; their license texts are preserved in licenses/single-instance-* and licenses/tray-icon-*. Cargo.lock pins their resolved versions.

## Policy profile assets

Mihomo 1.19.33 is distributed as a separate, unmodified upstream
executable under GPL-3.0. Its [corresponding source and build definitions](https://github.com/MetaCubeX/mihomo/tree/v1.19.33),
[source archive](https://github.com/MetaCubeX/mihomo/archive/refs/tags/v1.19.33.tar.gz)
and [upstream binaries](https://github.com/MetaCubeX/mihomo/releases/tag/v1.19.33)
are available from MetaCubeX. The bundled
`licenses/Policy-engine-notices.tar.gz` records the Go module versions,
checksums, Go runtime notices and dependency license texts for the exact
Windows and Linux executables.

GeoIP data comes from [V2Fly GeoIP 202611080027](https://github.com/v2fly/geoip/releases/tag/202611080027)
under CC BY-SA 4.0 and includes [IP Geolocation by DB-IP](https://db-ip.com/)
under CC BY 4.0. GeoSite data comes from
[V2Fly domain-list-community 20261118141523](https://github.com/v2fly/domain-list-community/releases/tag/20261118141523)
under MIT. Moth redistributes both datasets without modification under
the runtime filenames GeoIP.dat and GeoSite.dat. Their hashes and origins
are in `policy-assets.lock.json`.
