# Status — 2026-11-13

Version 0.3.3 candidate: service/core diagnostics, plain interface text and staged manual service upgrades.
Publication remains gated on target-platform acceptance; the checks below distinguish passed tests from unresolved checks.

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

## 0.3.3 verification and publication gate

- Rust workspace unit/integration tests, fmt and Clippy with warnings denied pass. System-network tests are excluded from host execution.
- TypeScript/Vite and ten Playwright scenarios pass, including IPC v2 service 0.3.1 and clearing stale diagnostics.
- Cross-compilation checks pass for moth-core, moth-platform and moth-ipc on x86_64-unknown-linux-gnu. The full Linux engine/service CI job is configured but has not run for this candidate.
- Disposable Windows 10 x64 VM: clean installation, updates from both 0.3.1 and 0.3.2, modified-core/unsafe-ACL refusal, forced post-start failure rollback, repeated/stopped update, remove/reinstall and unchanged decryptable DPAPI data pass.
- The pinned previous-release download script passes SHA-256 verification. New upgrade acceptance is configured in Windows CI; configuration is not a passing CI result.
- Disposable VM: ordinary-user authentication passes. External HTTPS/DoH reachability passes, but the service-backed VPN fixture fails its HTTPS connection check on both candidate 0.3.3 and baseline 0.3.2. The separate TUN test reaches the IPv4 fixture, then fails the IPv6 fixture with HostUnreachable.
- Therefore real service VPN/native tray acceptance is NOT a passing 0.3.3 result. Windows 10 is outside the initial support matrix; target Windows 11 acceptance and fresh Windows/Linux CI are required before publication. Previous 0.3.1/0.3.2 evidence below is historical, not substituted for this gate.
- R0 remains partial: stage-specific failures, redacted diagnostic journal/export, tray Connect, signed automatic updates and power-loss recovery are pending.

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
Linux retains compiling common crates; daemon, storage and TUN integration remain pending.
Native GUI DPI/accessibility acceptance and production signing remain pending.
