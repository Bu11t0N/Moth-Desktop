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

- Graphical Windows NSIS installation, legacy 0.3.1/0.3.2 upgrades, running/stopped post-start failure rollback, retry, shortcuts, uninstall and byte-identical decryptable DPAPI retention passed on [7e728d5](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43648712358), [5d937fc](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43649957129) and [66eb8f7](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43651283669). The protected full-tree transaction preserves modified/unknown files and backups. Manual service-only installation remains supported; graphical installations use the graphical installer for later maintenance. A named mutex serializes package transactions.
- Windows and Linux now have a protected kernel maintenance lease. Each tunnel retains a shared lease through startup and its lifetime; exclusive update maintenance cannot interrupt an active connection, including another account's. Actual shared/exclusive contention passed in [2002f50 Windows and Linux acceptance](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43721714786); that run did not pass the complete Linux acceptance.
- Linux uses a fixed root-authenticated Unix socket, kernel-derived UID ownership, reduced-capability systemd daemon, pinned cores, a private TUN/routing table and link-only resolved DNS. Secret Service has no file/plaintext fallback. Actual isolated keyring roundtrip, replacement, corruption rejection and locked-provider refusal passed on [3903f00](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43643517819) and subsequent Linux runs. Ubuntu 24.04 with systemd-resolved and a password-protected Secret Service provider is the initial target; other distributions remain unaccepted.
- Linux GTK/WebKit compilation, workspace Clippy, ordinary-user daemon authorization, actual HTTPS, DNS and TCP/UDP over IPv4/IPv6 TUN passed in [2002f50](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43721714786). Exact teardown failed. [abab5d9 diagnostics](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43722719857) identified sing-tun's additional IPv6 output-interface rule, created even with auto_route disabled. The journal now records its precise priority/private table before startup and cleanup refuses nonmatching rules. Cancellation waits for process teardown before journal recovery. The complete network/cancellation/crash/restart and native lifecycle step subsequently passed on [33d6373](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43725276528).
- Linux GUI integration uses the authenticated client and confirmed Exit barrier. Close hides only with a registered StatusNotifier host; otherwise it keeps a minimized taskbar window. Losing the host restores a hidden window. A native Exit button is available without a tray. Actual ordinary-user close with a live VPN, second-instance reopen and confirmed Exit passed with an XFCE tray host and separately with the taskbar fallback on [33d6373](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43725276528). No Linux user release is claimed.
- The `.deb` candidate uses standard Debian packaging/systemd helpers and preserves user-keyring data through removal/purge. The first .deb built successfully, but clean apt installation failed because the default product-named resource directory differed from the fixed daemon layout. Linux packaging now uses a consistent product/resource directory, a friendly desktop entry and pre-install archive path/ownership checks. Fresh apt install/remove/reinstall and live Secret Service retention acceptance remain pending. Package build/acceptance can run after network-test failure when compilation/lint passed; this does not turn an unsuccessful Linux network run into a passing check. [Supported candidate scope](LINUX.md).
- Signed-update GUI and fixed privileged maintenance are an implementation candidate: actual byte progress, monotonic signed metadata, exact repository URLs, independent copied-package signature/hash and embedded-version checks, confirmed own-VPN stop, UAC/polkit authorization, complete NSIS or apt/dpkg transaction, rollback verification and ordinary-user monitor/restart. Linux uses a protected dpkg-repack backup. Windows uses a separate versioned protected helper to avoid executable replacement locks. Private keys are outside Git; signing secrets are in the release environment. Updater signing is distinct from absent Windows Authenticode. [Update architecture](UPDATES.md).
- Windows workspace compilation/Clippy, Linux helper cross-compilation/Clippy, TypeScript build and eleven UI scenarios pass locally. Official Tauri signer fixtures reject changed/unsigned/wrong-key/old/incompatible metadata/packages. UI tests cover metadata escaping and no installation after a signature error. Installed privileged-verifier rejection tests are configured in disposable CI. Actual two-build signed installation/rollback/retry, native update monitor acceptance and production release publication remain pending; no released update channel is claimed.
- [33d6373 Windows and NSIS](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43725276528) passed the complete Windows suite and actual installer acceptance with the protected updater installed, including privileged unsigned-input refusal. Separate two-build native signed-update jobs now build only temporary 0.3.2/0.3.3 test packages with an independent ephemeral key and a local HTTPS fixture trusted through the guest OS. They exercise real ordinary-user TUN before update, changed metadata/package rejection without interrupting VPN, confirmed VPN stop, a post-start installer fault, actual rollback, retry, profile retention and ordinary-user monitor/restart. These new jobs are configured, not yet passed. Test binaries/keys are neither uploaded nor saved into shared build caches.
- [22898d1](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43727829462) and [d7248c7](https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43728571532) passed the complete Linux network/native GUI and actual Debian install/remove/reinstall/purge/keyring-retention checks. The first signed Linux run rejected changed metadata/package and reopened the retained old version, but its success retry failed. Durable installation/rollback phase evidence and a post-start marker now prevent a preparation refusal from being mistaken for rollback; failed credential-free package-manager output is retained for diagnosis. Windows signed acceptance initially failed before native builds because Python used an ANSI source encoding; explicit UTF-8 is now required. Full signed-update acceptance remains open.

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
Linux candidate daemon, keyring, TUN, native desktop lifecycle and Debian package acceptance have passed on Ubuntu 24.04; production publication and signed-update acceptance remain pending.
Native GUI DPI/accessibility acceptance and production signing remain pending.
