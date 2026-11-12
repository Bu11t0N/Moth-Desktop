# Moth Desktop — Мотылёк

Windows-first desktop companion to [Moth for Android](https://github.com/Bu11t0N/Moth).
Rust domain/platform layers and a Tauri 2 + TypeScript interface, designed for a later Linux port.

**Development preview. This version does not establish a VPN and does not protect traffic.**

## Implemented

- Local VLESS/Trojan URI and Base64-list import with explicit validation and atomic batches.
- TCP, WebSocket, gRPC and XHTTP profile representations; TLS and VLESS Reality/Vision.
- Deduplication, server selection/removal, credential-free UI summaries.
- Windows account-bound DPAPI storage with atomic replacement. No plaintext/localStorage fallback.
- Desktop interface: Home, Servers, Settings, import and delete dialogs.
- Pinned Xray 26.10.21 and sing-box 1.12.25, archive/executable SHA-256 verification.
- Real per-server HTTPS probe through a short-lived authenticated local Xray proxy.
- Engine-validated configuration matrix and sing-box TUN plan (not executed as a VPN).
- Windows Job Object cleanup, bounded process lifetimes, config through stdin, suppressed core logs.
- Windows Service + authenticated/versioned named pipe, optional service-backed probes.
- Separate service health view; explicit manual installation, no automatic privilege elevation.

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
cargo build -p moth-service --release
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
npm run build
npm test
npm run desktop:build -- --no-bundle
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

## Optional Windows service (0.3.0 development preview)

The fully extracted development ZIP includes `moth-service.exe` and `Service.ps1`.
Normal use does not install a service. See [service architecture, manual setup and limits](docs/SERVICE.md).
The service supports health/probes only; it does not yet enable a system-wide VPN.
SCM acceptance runs only on disposable CI, not on the developer's host computer.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE) and [THIRD_PARTY.md](THIRD_PARTY.md).
