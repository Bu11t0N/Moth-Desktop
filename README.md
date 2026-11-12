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
- Xray configuration generation (currently internal, tested structurally; not yet engine-validated).

Unsupported URI options are rejected rather than silently discarded. HTTP subscription fetching,
Clash/sing-box/Xray JSON imports, Android backups and other protocols are planned, not implemented.
Linux can compile the common logic and UI but saving profiles returns an explicit unsupported error.

## Develop

Install Node.js 22.12+ and stable Rust (MSVC target on Windows), Microsoft C++ Build Tools and WebView2.
See [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

```sh
npm ci
npm run desktop:dev
```

```sh
cargo test --workspace
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
npm run build
npm test
npm run desktop:build -- --no-bundle
```

`npm run dev` opens a browser-only UI preview: no persistence, fake servers or simulated connection.
`npm test` requires Playwright Chromium (`npx playwright install chromium`). Tests exercise the UI;
Rust tests cover parsing, configuration, library invariants and Windows DPAPI.

Debug builds accept `MOTH_DEV_DATA_DIR` to isolate test profiles. Release builds ignore it.
Normal Windows data lives in Tauri's local application-data directory for `org.mothvpn.desktop`.
DPAPI is bound to the Windows account; copying the encrypted file is not a portable backup.
Corrupt/undecryptable storage causes an explicit startup failure and is never overwritten automatically.

## Project

```
crates/moth-core/       UI-independent profiles, import and config generation
crates/moth-platform/   Windows protected storage, platform capabilities
src-tauri/             Tauri adapter and narrowly scoped IPC commands
src/                   Local-only desktop interface
docs/                  Specification, architecture and progress
tests/                 Browser UI checks
```

The domain/platform crates do not depend on Tauri. A later Iced frontend can reuse them;
shipping two frontends is outside the first release. No Android repository files are changed.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE) and [THIRD_PARTY.md](THIRD_PARTY.md).
