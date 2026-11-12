# Status — 2026-11-12

Current milestone: M1b control plane, preview 0.3.0. No system VPN yet.

## Implemented

- M0/M1a: profiles, protected Windows storage, Tauri UI, pinned cores, real HTTPS probes, native config checks and isolated loopback VLESS tests.
- moth-ipc: bounded/versioned frames, strict request schemas, response correlation, authenticated Windows named pipe and explicit development-only transport.
- moth-service: real SCM dispatcher, stop/shutdown lifecycle, protected installation gate, health/probe commands and bounded concurrency.
- Production client verifies SCM PID and LocalSystem before sending credentials. Server checks the actual caller token, integrity, AppContainer and group membership.
- Explicit manual install/remove script with fixed Program Files layout and ACLs.
- GUI service health is separate from VPN status. Probes use the installed service when available; preview local probes remain when it is absent. Auth errors never fall back.

## Verification

- Prior 0.2.0 implementation 9a5e5fe: Windows/Linux CI passed: https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43228887516
- Current local Windows: 19 Rust tests passed (10 core, 3 engine, 3 protocol, 2 real pipe integration, 1 DPAPI); Clippy with warnings denied passed.
- Five Playwright scenarios and TypeScript/Vite build passed.
- Console process/pipe integration covers health, first-instance protection, wrong version, oversized request, recovery, invalid profile and rejection of a fake peer before credentials are written. PowerShell scripts parse successfully.
- Real SCM install/start/authenticated health/stop/restart/remove added to disposable Windows CI only. Current commit SCM/CI result must be recorded after publishing.
- No service was installed and no network settings changed on the developer host.
- Native WebView2 live-window/DPI/accessibility checks remain pending; previous automation app-launch approval timed out. Browser UI tests are not native visual acceptance.

## Next

1. Interface discovery and upstream socket/route exclusion, then actual TUN/DNS in a VM.
2. Per-user connection ownership, lifecycle/recovery policy and OS kill switch.
3. Non-admin/multiple-session and low-integrity/AppContainer acceptance; security review.
4. HTTPS subscriptions, Android backups, tray and signed installer/update policy.
5. Linux daemon/transport/secret storage. Protocol/domain/engine logic remains portable.

## Not implemented / not claimed

System-wide VPN connection, working TUN, kill switch, DNS/IPv6 leak protection, persistent tunnel/reconnection, production installer/upgrades, automatic updates, Linux daemon/storage, full protocol parity or native GUI acceptance. A RUNNING service or a successful HTTPS probe does not protect system traffic.
