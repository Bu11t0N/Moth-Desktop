# Status — 2026-11-15

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

- Signed-update verification foundation now has a compiled public trust anchor, bounded signed metadata and streaming package signature/hash verification, exact repository URLs, protocol checks and monotonic versions. Official Tauri signer fixtures pass tampering, unsigned/wrong-key and rollback refusal tests. Private signing material is outside the repository; the release environment holds signing secrets. This is updater signing, not Windows Authenticode. GUI installation and privileged update/rollback are not implemented yet.
- A protected kernel maintenance lease coordinates tunnel startup/lifetime with exclusive maintenance across service restarts. Initialization is an explicit administrator/root installation operation; real active-TUN contention checks are configured in disposable CI. The new lease has compiled on Windows and Linux; passing runtime acceptance is not yet claimed.
- [66eb8f7 CI](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43651283669) again passed Windows and NSIS. Linux ordinary-user authentication and real HTTPS connection through TUN now pass after keeping the fixture user manager alive. Subsequent isolated TCP acceptance timed out; full Linux VPN/native/package acceptance remains unresolved. The fixture now checks its isolated servers before VPN startup and records bounded network diagnostics.

- Graphical Windows NSIS packaging is implemented as a development candidate: one protected staging transaction for GUI/service/pinned cores, all-user shortcuts and uninstall registration. The GUI runs without elevation. Uninstall preserves per-user profiles, modified/unknown files and previous installation backups. Existing manual service-only installation remains supported; graphical installations require the graphical installer for further updates. Actual installer acceptance is pending; do not substitute the previous manual-service acceptance for it.
- Expanded release work remains pending: functioning Linux daemon/storage/IPC/TUN and `.deb`, signed in-app updates on both platforms, coordinated maintenance and actual two-build update/rollback acceptance, signed release metadata and installer publication. Ubuntu 24.04 with systemd-resolved and Secret Service is the initial Linux target; other distributions require separate acceptance.
- Linux foundation: fixed Unix socket client with root peer authentication, UID-based daemon authorization and fixed installation ownership checks; Secret Service persistence with encrypted session and no file/plaintext fallback. Linux-target compilation and Clippy pass. Actual isolated Secret Service roundtrip/replacement/corruption/locked-provider acceptance is configured but pending; daemon/TUN/GUI support is still incomplete.
- Linux service/TUN candidate is implemented but awaits actual network acceptance: root-owned daemon/socket with bounded fixed IPC, shared connection controller, pinned engines, upstream mark/interface binding, private `moth0` link, dedicated policy routing and link-only resolved DNS. A root-only journal records rule ownership before activation; teardown/startup/ExecStopPost remove only exact owned rules and retain conflicting state for repair. Engines receive parent-death SIGKILL and systemd cgroup containment. Linux-target compilation/Clippy pass. The hosted Linux acceptance now exercises real UID isolation, HTTPS/DNS, TCP/UDP IPv4/IPv6, engine death and daemon SIGKILL/restart; no result is claimed until it runs. GUI and `.deb` integration are still pending.
- The actual isolated Secret Service acceptance passed on [3903f00 Linux CI](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43643517819), including replacement, corruption rejection and locked-provider refusal. This is storage/core evidence, not Linux VPN evidence.
- Linux GUI integration candidate now uses the authenticated Unix client, platform resources and the common confirmed-Exit barrier. Close hides only when a real StatusNotifier host is registered; otherwise it keeps a minimized taskbar window. Loss of the tray host restores a hidden window. A native Exit button is available without a tray. Windows compilation/Clippy and ten UI tests pass; Linux GTK/WebKit compilation and native acceptance with/without a real XFCE tray host are pending in hosted CI. No Linux user release is claimed.
- Initial real NSIS acceptance failed because Windows PowerShell inherited PowerShell 7 module paths. Maintenance now restricts module lookup to the current PowerShell and trusted Windows system modules; the rollback fixture requires the final started-service health failure, not merely any nonzero installer exit. Fresh installer acceptance is pending. Upgrade baseline downloads use fresh runner-temporary directories, outside restored Cargo caches.
- Linux `.deb` build/maintenance scripts are implemented through Tauri's standard Debian packager and systemd helpers, with root-controlled program files and user-keyring retention even on purge. Actual apt install/remove/reinstall plus live Secret Service fixture verification are configured but pending. [Linux candidate scope and requirements](LINUX.md) remain explicitly limited; signed in-app updates are not implemented.
- [7e728d5 Windows CI](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43648712358) passed both the existing real Windows suite and actual graphical NSIS install/legacy upgrades/running-and-stopped rollback/retry/uninstall/DPAPI retention. A second [5d937fc Windows and NSIS run](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43649957129) also passed. Linux GTK/WebKit compilation, Clippy and real Secret Service checks pass; network/native lifecycle acceptance remains unresolved. Diagnostics showed that short PAM test sessions removed the fixture's runtime directory; the fixture now keeps a real systemd user manager with linger rather than weakening UID authorization.

- Fresh implementation f62da4c: [Windows and Linux CI passed](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43505351323). Windows runner: Windows Server 2025; Linux runner: Ubuntu 24.04. These are fresh candidate results, not substituted 0.3.2 evidence.
- Rust workspace tests, fmt, Clippy with warnings denied, TypeScript/Vite and ten Playwright scenarios pass. The UI scenarios include service loss after successful health and rejection of a delayed old health response. Health diagnostics do not replace service-owned tunnel state.
- Full Linux common-crate/engine/service tests, including the pinned-engine checks, pass. Local cross-compilation also passes for moth-core, moth-platform and moth-ipc on x86_64-unknown-linux-gnu.
- Windows CI: real TUN TCP/UDP over IPv4/IPv6, disconnect/reconnect, adapter teardown and preservation of pre-existing physical DNS/routes pass.
- Windows CI: clean installation, upgrades from both 0.3.1 and 0.3.2, modified-core/unsafe-ACL refusal, forced post-start failure rollback of running and stopped services, repeated/stopped update, remove/reinstall and unchanged decryptable DPAPI fixtures pass. The pinned previous-release download script verifies SHA-256. Modified-core fixtures copy only the three service/core files, not Cargo build directories.
- Windows CI: standard-user authentication and real service-backed HTTPS/DNS, SID isolation, reconnect, engine crash and service shutdown pass. Native GUI under the standard user passes tray registration, real close-to-tray with live VPN, second-instance reopen and confirmed Exit disconnection. Explicit debug storage is resolved before querying shell folders for a newly created test account; release storage remains Windows account-bound.
- Release EXEs are built with compiler-path remapping and stripped debug data. Packaging checks component versions/core pins and produces SHA256SUMS. No system-network tests are run on a normal workstation.
- Separate Windows 10 VM observations remain unresolved: the service HTTPS fixture failed for both candidate 0.3.3 and baseline 0.3.2; a local TUN fixture reached IPv4 and failed IPv6 with HostUnreachable. This is neither proof of a 0.3.3 regression nor passing acceptance on that VM.
- **Publication gate remains open:** the expanded installer/Linux/updater scope has not passed acceptance. A separate Windows 11 check is not a release requirement; it has not been performed and is not claimed. R0 remains partial: stage-specific failures, redacted diagnostic journal/export, tray Connect, signed automatic updates and power-loss recovery are pending.

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
Linux has a compiling transport/storage foundation; real keyring acceptance, daemon, TUN and desktop integration remain pending.
Native GUI DPI/accessibility acceptance and production signing remain pending.
