# Moth Desktop — Мотылёк

Windows-first desktop companion to [Moth for Android](https://github.com/Bu11t0N/Moth).
Rust domain/platform layers and a Tauri 2 + TypeScript interface, designed for a later Linux port.

**0.3.3 development candidate: Windows system TUN connection. Requires the installed service. Kill switch and automatic network recovery are not implemented.**

See the [desktop roadmap and Android feature comparison](docs/ROADMAP.ru.md)
for the planned implementation order and acceptance criteria.

## Implemented

- Local VLESS/Trojan URI and Base64-list import with explicit validation and atomic batches.
- TCP, WebSocket, gRPC and XHTTP profile representations; TLS and VLESS Reality/Vision.
- Deduplication, server selection/removal, credential-free UI summaries.
- Windows account-bound DPAPI storage with atomic replacement. No plaintext/localStorage fallback.
- Desktop interface: Home, Servers, Settings, import and delete dialogs.
- Pinned Xray 26.10.21 and sing-box 1.12.25, archive/executable SHA-256 verification.
- Real per-server HTTPS probe through a short-lived authenticated local Xray proxy.
- Engine-validated configuration matrix and an executable sing-box TUN / Xray bridge.
- Windows Job Object cleanup, bounded process lifetimes, config through stdin, suppressed core logs.
- Windows Service + authenticated/versioned named pipe, optional service-backed probes.
- Connect/disconnect via authenticated service, per-account connection ownership, engine monitoring.
- Native tray with open/disconnect/exit, close-to-tray, single instance and verified disconnect before Exit.
- GUI 0.3.3 remains compatible with installed service 0.3.1 (IPC v2); no service upgrade is needed for tray support.
- Explicit Install-Service.cmd bootstrap with Windows UAC; the GUI runs as an ordinary user.
- Service/core version diagnostics with authenticated IPC compatibility and current service core hash checks.
- Staged 0.3.1/0.3.2 service upgrades, startup verification, automatic failure rollback and unchanged DPAPI profiles.

Unsupported URI options are rejected rather than silently discarded. HTTP subscription fetching,
Clash/sing-box/Xray JSON imports, Android backups and other protocols are planned, not implemented.
Linux can compile the common logic and UI but saving profiles returns an explicit unsupported error.

## Develop

Install Node.js 22.12+ and stable Rust (MSVC target on Windows), Microsoft C++ Build Tools and WebView2.
See [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

```sh
npm ci
npm run engines:fetch
npm run desktop:dev
```

```sh
cargo test --workspace
npm run engines:test
cargo test -p moth-ipc -p moth-service
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
npm run build
npm test
./scripts/build-windows.ps1
```

`npm run dev` opens a browser-only UI preview: no persistence, fake servers or simulated connection.
`npm test` requires Playwright Chromium (`npx playwright install chromium`). Tests exercise the UI;
Rust tests cover parsing, configuration, library invariants and Windows DPAPI.
`npm run engines:test` additionally runs the real engines against a local-only VLESS server,
checks incorrect credentials and validates the 10-row configuration matrix. These checks
are run on both Windows and Linux in CI; they never modify host routes, DNS or firewall.

On Servers, **Проверить сервер** requests `https://cp.cloudflare.com/generate_204` through
the selected server. DNS for that request is delegated through SOCKS5h. HTTPS certificates
are verified, redirects are rejected, no direct fallback is used. The displayed time covers
the request (including upstream setup), excludes local core startup and is not ICMP ping.
Success does not enable VPN protection. See [engine details and limitations](docs/ENGINES.md).

Debug builds accept `MOTH_DEV_DATA_DIR` to isolate test profiles. Release builds ignore it.
Normal Windows data lives in Tauri's local application-data directory for `org.mothvpn.desktop`.
DPAPI is bound to the Windows account; copying the encrypted file is not a portable backup.
Corrupt/undecryptable storage causes an explicit startup failure and is never overwritten automatically.

## Project

```
crates/moth-core/       UI-independent profiles, import and config generation
crates/moth-platform/   Windows protected storage, platform capabilities
crates/moth-engine/     Verified core processes, config validation and server probes
crates/moth-ipc/        Portable protocol and Windows authenticated named pipe
crates/moth-service/    Windows SCM executable and non-privileged console test harness
src-tauri/             Tauri adapter and narrowly scoped IPC commands
src/                   Local-only desktop interface
docs/                  Specification, architecture and progress
tests/                 Browser UI checks
```

The domain/platform crates do not depend on Tauri. A later Iced frontend can reuse them;
shipping two frontends is outside the first release. No Android repository files are changed.

## Windows service installation and update

The fully extracted development ZIP includes `moth-service.exe` and `Service.ps1`.
Run Install-Service.cmd to install or update the service and pinned engines. Disconnect VPN and close the previous GUI using tray Exit first. See [installation, automatic rollback and limitations](docs/SERVICE.md).
SCM/network acceptance runs only on disposable Windows CI or an explicitly selected VMware clone. `-DisposableVm` checks VMware hardware before enabling the debug/test drivers. Never run these tests on a normal workstation.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE) and [THIRD_PARTY.md](THIRD_PARTY.md).
