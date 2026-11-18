# Status — 2026-11-17

## 0.3.5 in development

- Persistent egress guard groundwork uses an isolated WFP provider/sublayer on Windows and an owned nftables table on Ubuntu. Typed owner controls, pinned endpoint cache, explicit recovery CLI and early Ubuntu restore unit are implemented. Disconnect/Exit retain the opt-in block; uninstall explicitly releases only Moth rules. Direct rules and application splitting are rejected while enabled.
- Local Windows compile/Clippy, Linux service cross-check and targeted policy/confirmation/English UI tests pass. Kernel filtering, crash/reboot behavior, upgrade/uninstall and absence of leaks are **not yet accepted**. No persistent/power-loss protection claim is made from these local checks.
- Measured engine traffic/connections and bounded opt-in history, typed memory-only diagnostics with previewed export, Settings/Diagnostics/About and compiled license documents are implemented. Polling stops on hidden/unfocused windows. These are engine counters, not billing or whole-adapter measurements.
- Ordinary-user login registration is separate from automatic connection. Owned HKCU Run or Linux desktop entries can be updated; foreign changes are preserved. Registration does not bypass OS startup restrictions.
- Authenticated mixed HTTP/SOCKS proxy-only sessions create no TUN. Optional TUN sharing and explicit private-interface LAN allowlists use the same pinned engines. Legacy IPC never reports a proxy session as system VPN. Native subscription/update downloads use an explicit private upstream channel and retain blocked intent after proxy-only failures. Per-user WinINet/GNOME proxy restoration uses a protected ownership journal; these OS changes still require disposable native acceptance.
- A real loopback-only engine test passed authenticated HTTP and SOCKS, rejected missing/wrong proxy passwords, measured nonzero traffic and reaped both processes. Windows workspace Clippy, Linux service/platform cross-check and targeted UI checks passed. LAN clients, OS proxy/startup registration, persistent firewall crash/boot behavior and package lifecycle remain unaccepted. Hysteria2 (including salamander), TUIC v5 and AnyTLS URI and strict supported sing-box JSON/Clash extraction are implemented through the pinned sing-box binary. Native configuration checks and real loopback TLS/authentication tests passed for all three families, including wrong-password and wrong-certificate-name rejection. Port hopping, arbitrary raw engine options and other Android-only families are not supported. Final packaging remains in progress. No 0.3.5 release is published.

Version 0.3.4 was published from `9c792b62cd9152a4d2627123392f7a9ab613a6bc`. [All five native jobs](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43905111907) and [production signing, Linux verification and publication](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43909590598) passed. Downloaded release checksums, signed metadata and all three package signatures were independently verified.

## 0.3.4 candidate

- Windows installer source is explicitly read as UTF-8. The license page uses a BOM-marked UTF-16LE build copy with CRLF, preserving the source text.
- Added actual installer/uninstaller page text checks and screenshot artifacts under English and Russian user cultures. These checks do not change the installer language or system codepage.
- Added the published 0.3.3 Windows package to installer upgrade/data-retention acceptance.
- Candidate implementation adds protected multi-source subscriptions, per-source device-data consent, a random persistent desktop installation ID, bounded HTTP(S) downloads, origin-scoped headers, quota metadata and manual/scheduled/VPN-only refresh. Actual paid-provider account compatibility has not been tested.
- Candidate library controls include search, favorites, sorting, archived selected/favorite servers and sequential measured batch probes. VMess AEAD, Shadowsocks AEAD/2022, SOCKS5 and HTTP(S) join the existing VLESS/Trojan families, with typed supported JSON/Clash extraction. Unknown or lossy server options fail the entire import.
- Candidate connection policies include DNS/bootstrap, MTU, IPv6 rejection, ordered routing and executable-path splitting. Service recovery, HTTPS health checks and bounded candidate failover are opt-in. These policies require service 0.3.4; old default Connect remains IPC v2 compatible. New privileged network behavior still requires isolated native acceptance.
- Candidate backups use AES-256-GCM and PBKDF2-HMAC-SHA256 from established libraries. Native preview returns counts only; merge/replace confirmation and session undo reject stale or expired transactions. Android v1 encrypted/plain backups import compatible profiles, favorites, selection and ordered rules. Phone HWID and package IDs are excluded; imported source consent/schedules and automatic connection are disabled. Android-specific settings and unsupported protocols are not silently imported.
- Candidate themes and Russian/English UI have browser-level coverage. Keyboard, screen reader and native Windows/Linux delivery acceptance remain separate.
- Local verification: 24 core tests, native stale/expired backup transaction test, workspace Clippy with warnings denied, Windows desktop compile, Linux service/platform cross-check, TypeScript/Vite build and 15 browser scenarios pass. Android encrypted format has an independent Node/OpenSSL fixture; this is not an Android device end-to-end test.
- Unfinished expanded scope includes persistent kill switch, engine connection/traffic telemetry, diagnostics export, further Android protocol families, explicit general proxy/LAN mode and OS login autostart. No claim of complete Android parity or release readiness is made.
- Native acceptance and publication of the expanded candidate are pending. Engine pins, production updater keys and release gates remain unchanged. No intermediate installer-only release is published.

Version 0.3.3 is [published](https://github.com/Bu11t0N/Moth-Desktop/releases/tag/v0.3.3) for Windows x64 and the supported Ubuntu 24.04 x64 configuration. It uses unmodified pinned Xray/sing-box engines. The limitations below remain applicable.

## Implemented

- Previous profile import, DPAPI, pinned engines, HTTPS probes and authenticated SCM service.
- System TUN using sing-box with embedded Wintun; IPv4/IPv6 routes and DNS on the private adapter.
- Xray upstream interface binding, server resolution before routing changes, authenticated local bridge.
- Connection success requires HTTPS sourced from the TUN address. No direct/proxy fallback.
- IPC v2 typed connect/disconnect/status, authenticated SID ownership, startup cancellation and bounded startup.
- Service owns the connection independently of the GUI and monitors both engines every 500 ms.
- Connect/disconnect UI polls service state; service loss removes connected indication.
- Explicit service installation bootstrap and versioned Windows ZIP packages.

- Added native tray, close-to-tray, single-instance reopen, native status polling and graceful Exit with confirmed owner disconnection.
- Added a Connect/Exit barrier and strict SCM stopped-state check; no service protocol change. Installed service 0.3.1 is compatible.

## 0.3.3 implemented

- Plain screen/action/state names; connection headings reflect service-owned phases.
- GUI/service/core versions, IPC v2 compatibility and separate build/runtime capabilities.
- Service health rechecks pinned core hashes; service loss clears positive diagnostics and invalidates pending health responses by generation.
- Existing service 0.3.1 remains compatible. No new IPC commands or arbitrary runtime engine/config paths.
- Manual 0.3.1/0.3.2/0.3.3 service upgrades validate SCM configuration and existing ACLs, stage pinned cores before stop, verify the candidate and restore the previous files/state on startup failure.
- DPAPI profiles are untouched by maintenance. Remove/reinstall needs no manual Program Files cleanup; previous/failed files are preserved for review.
- Release build script remaps compiler paths, strips debug information and scans EXEs; packaging checks component versions and core hashes.

## 0.3.3 verified release

- Runtime source/tag: `e5aa7feee56afc07be56e63e4309d2831740804e`, `v0.3.3`.
- [All five native CI jobs passed](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43746501726): Windows, Linux, graphical installer and signed updates on each OS. Windows guest: Windows Server 2025; Linux guest: Ubuntu 24.04. A separate Windows 11 check was waived and is not claimed.
- Windows NSIS: complete GUI/service/pinned-core installation, ordinary-user shortcuts, legacy 0.3.1/0.3.2 upgrades, running/stopped post-start failure rollback, retry and uninstall. DPAPI bytes remain unchanged and decryptable. Unknown/modified files and backups are retained. WebView2 is a prerequisite.
- Windows service: real TUN TCP/UDP over IPv4/IPv6, HTTPS/DNS, SID isolation, cancellation/reconnect, engine/service failure and preservation of existing physical DNS/routes. Actual ordinary-user tray registration, close with a live VPN, second-instance reopen and confirmed Exit pass.
- Ubuntu 24.04: real password-protected Secret Service persistence, locked/corrupt-provider refusal, UID-authenticated daemon ownership, HTTPS/DNS/TCP/UDP over IPv4/IPv6 TUN, exact route/rule/resolver restoration, startup cancellation, engine death, SIGKILL and service recovery. Native ordinary-user close/reopen/Exit pass with an XFCE tray host and separately with taskbar fallback. Other distributions remain unaccepted.
- Debian package: fixed protected GUI/daemon/helper/core/unit paths, actual apt install/remove/reinstall/purge, healthy systemd service and retained live keyring data. apt/dpkg owns executable replacement.
- On both OSes, two temporary native builds of the new updater (0.3.2 to 0.3.3) pass changed-metadata rejection, corrupt-package rejection without interrupting VPN, confirmed own-VPN stop, an actual post-start installation fault, verified rollback and successful retry. Actual download progress, ordinary-user monitor/restart, component health, profile retention and a real VPN before/after pass. These temporary updater baselines do not add an updater retroactively to the published 0.3.2.
- Root independently verifies signatures, monotonic version/protocol/repository policy and copied-package bytes. Actual kernel maintenance contention blocks replacement during an owned tunnel; ordinary users cannot read unverified private staging. Installed unsigned-input refusal and official signer vectors for wrong keys, changed bytes, old/incompatible versions and signed-version mismatch pass.
- Workspace tests/fmt/Clippy, TypeScript build and eleven browser UI scenarios pass. Native acceptance and production artifacts are separate from temporary test-key builds; test keys/binaries are never release assets or shared-cache uploads.
- [Production signing, Windows/Linux offline verification and publication passed](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43748619093). Published files: Windows ZIP and NSIS, Linux .deb, their signatures, signed bounded metadata, full instructions and [SHA256SUMS](https://github.com/Bu11t0N/Moth-Desktop/releases/download/v0.3.3/SHA256SUMS). The public tag points to the accepted source. Existing versions/assets are not overwritten.
- All published files were downloaded again: all nine checksum entries, metadata and all three package signatures verify with the compiled production public key. Signing keys/passwords remain outside Git and assets. Updater signing is distinct from Windows Authenticode, which is absent.
- Complete Rust/JavaScript notices and engine dependency notices ship in the packages. All 149 distinct engine dependency module sums were checked against the four pinned upstream binaries. No engine protocol/cryptography implementation was rewritten.

## 0.3.2 verification

- Local Rust tests pass, including two new tray shutdown tests (confirmation, IPC failure and other-account isolation).
- Formatting, Clippy with warnings denied, TypeScript/Vite and seven Playwright tests pass.
- Implementation 2512223: Windows and Linux CI passed: https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43450775552
- Disposable Windows VM: native tray registered; real VPN remains connected after the native window close event; a real second GUI process exits and restores the same window; the Exit menu handler confirms disconnection and leaves no active TUN adapter.
- Native lifecycle acceptance is debug-only and not compiled into release builds.
- GUI 0.3.2 works with installed service 0.3.1 (IPC v2); no service upgrade is required.
- 0.3.2 ZIP SHA-256: 84b67ac6d5776cf81c097317874f67d497d09e2e263b99673482f9caaf12ad69.

## 0.3.1 verification

- Implementation adc4f2c: Windows and Linux CI passed: https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43434893973
- Disposable Windows VM: real TCP/UDP over IPv4/IPv6 through TUN, disconnect/reconnect, teardown and preservation of physical adapter DNS/routes.
- Installed SCM service: separate non-administrator account connects through the real HTTPS verifier; explicit DNS query to the TUN gateway succeeds.
- SID isolation rejects another account's disconnect. Reconnect, forced Xray termination, failure reporting, recovery and service shutdown/restart pass.
- Native config matrix, loopback credential failure, DPAPI, actual pipe spoof/version/frame handling, ownership unit test, formatting and Clippy pass.
- Seven Playwright scenarios and TypeScript/Vite build pass, including cancellation and loss of service. These are browser adapter tests, not native WebView2/DPI acceptance.
- Windows EXE version and packaged file checksums verified for 0.3.1.
- Final ZIP SHA-256: 3c9155623b46ab42533eb8d2fe070986bb1d0cdbc185f162c12d207639460816.

## Limitations

No persistent kill switch: engine/service failure tears down the tunnel and normal networking returns.
No automatic reconnect after sleep/network switch; disconnect and connect again.
DNS on physical adapters is not rewritten. LAN/more-specific existing routes can bypass TUN; applications with explicitly bound adapters are outside the current guarantee.
No claim of comprehensive IPv6/DNS leak protection or compatibility with concurrent VPNs.
Linux daemon, keyring, TUN, native desktop lifecycle, Debian package and signed-update acceptance have passed on Ubuntu 24.04; other distributions remain unaccepted.
Native GUI DPI/accessibility and power-loss recovery acceptance remain pending. Windows Authenticode is absent.
