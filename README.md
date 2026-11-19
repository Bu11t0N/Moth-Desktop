# Moth Desktop — Мотылёк

Windows-first desktop companion to [Moth for Android](https://github.com/Bu11t0N/Moth).
Rust domain/platform layers and a Tauri 2 + TypeScript interface for Windows x64 and Ubuntu 24.04 x64.

**0.3.6** adds preview and execution of supported Clash/Mihomo policies.
See [Clash profiles and limitations](docs/CLASH.md) and the
[0.3.6 release packages](https://github.com/Bu11t0N/Moth-Desktop/releases/tag/v0.3.6).
The previous release is [0.3.5](https://github.com/Bu11t0N/Moth-Desktop/releases/tag/v0.3.5).

## Implemented

- Typed URI/Base64 and supported sing-box/Xray JSON/Clash server extraction;
  VLESS/Trojan, VMess AEAD, Shadowsocks AEAD/2022, SOCKS5/HTTP(S), Hysteria2,
  TUIC v5 and AnyTLS. Unknown or lossy options reject an import atomically.
- Bounded subscriptions, origin-scoped optional device consent, provider metadata,
  scheduled/VPN-only refresh, library search/favorites and real sequential probes.
- Windows DPAPI and password-protected Linux Secret Service; credential-free UI
  summaries and encrypted portable backups with compatible Android v1 migration.
- Pinned Xray 26.10.21 and sing-box 1.12.25 with compiled executable hashes.
  Configurations use private stdin; engine logs are suppressed.
- Authenticated per-account service lifecycle, real TUN/DNS startup verification,
  routing/DNS/MTU/IPv6/per-app policies and optional recovery/health/failover.
- Optional persistent owned WFP/nftables block, retained after Disconnect/Exit,
  with explicit release and administrator recovery. See [recovery](docs/SERVICE.md).
- Authenticated HTTP/SOCKS proxy-only mode, optional TUN sharing and explicit
  private-interface LAN allowlists. Proxy mode is distinct from system VPN.
  Per-user OS proxy changes have protected ownership restoration.
- Actual engine traffic/connections, opt-in bounded history, typed diagnostic
  preview/export, Settings/Diagnostics/About/licenses, Russian/English and themes.
- Native tray/single instance, separate ordinary-user login startup, staged
  Windows NSIS/ZIP and Ubuntu Debian installation, signed in-app updates.

New features require the coordinated 0.3.5 service. IPC v2 default connection
commands remain compatible with older services; they cannot enable new features.
No full Android/Clash/Mihomo/provider equivalence or universal power-loss/boot
guarantee is claimed. Proxy mode serves configured applications only. External
OS edits are preserved; check/restore the OS proxy after a crash before removal.
Linux support is limited to documented Ubuntu 24.04 x64 systemd-resolved and
Secret Service configurations. See [engines](docs/ENGINES.md),
[Linux support](docs/LINUX.md) and [updates](docs/UPDATES.md).

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
crates/moth-platform/   Windows DPAPI and Linux Secret Service storage
crates/moth-engine/     Verified core processes, config validation and server probes
crates/moth-ipc/        Authenticated Windows pipe/Linux socket and maintenance leases
crates/moth-service/    Windows SCM/Linux systemd daemon and bounded engine lifecycle
crates/moth-update/     Shared signed metadata/package verification
crates/moth-maintenance/ Fixed privileged signed-package installation
src-tauri/             Tauri adapter and narrowly scoped IPC commands
src/                   Local-only desktop interface
docs/                  Specification, architecture and progress
tests/                 Browser UI checks
```

The domain/platform crates do not depend on Tauri. A later Iced frontend can reuse them;
shipping two frontends is outside the first release. No Android repository files are changed.

## Windows service installation and update

The graphical NSIS installer installs the full GUI/service/core package with shortcuts and uninstall support. Its GUI requires Microsoft WebView2 Runtime and runs without elevation. Full graphical installation is required for in-app updates.
The fully extracted development ZIP also includes `moth-service.exe` and `Service.ps1`.
Run Install-Service.cmd to install or update the service and pinned engines. Disconnect VPN and close the previous GUI using tray Exit first. See [installation, automatic rollback and limitations](docs/SERVICE.md).
SCM/network acceptance runs only on disposable Windows CI or an explicitly selected VMware clone. `-DisposableVm` checks VMware hardware before enabling the debug/test drivers. Never run these tests on a normal workstation.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE) and [THIRD_PARTY.md](THIRD_PARTY.md).
