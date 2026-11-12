# Status — 2026-11-12

Current milestone: M1a, engine integration, preview 0.2.0. No system VPN yet.

## Implemented

- M0: Rust workspace, strict VLESS/Trojan/Base64 imports, deduplication, selection,
  Windows DPAPI storage and Tauri interface.
- `moth-engine`: pinned Xray 26.10.21 / sing-box 1.12.25, double archive/executable
  SHA-256 pins, runtime verification and no frontend-controlled executable paths.
- Timeout-bounded core processes, config via stdin, no retained core logs, restricted
  inherited environment and Windows Job Object cleanup.
- Per-server real HTTPS check via authenticated local SOCKS, UI latency/error reporting.
- sing-box TUN / authenticated Xray bridge config generation, validation only.
- Engine notices, exact source revisions and reproducible development fetch script.

## Verification

- M0 commit e624459: remote Windows and Linux CI both passed.
- Current local Windows tests: 13 Rust tests passed (9 domain, 3 engine, 1 DPAPI),
  including 10 native config variants, invalid-config rejection and real loopback VLESS
  traffic plus wrong-UUID failure. Clippy with warnings denied passed.
- Frontend build and 4 Playwright scenarios passed, including probe UI semantics.
- SHA-256 hashes measured for official Windows/Linux x64 archives/executables.
- Linux native-engine tests added to CI; current-commit result must be checked after push.
- Production UI screenshot and Windows EXE compilation checked at M0; M1a native
  WebView2 visual/runtime acceptance and additional DPI/accessibility checks remain pending.

## Next

1. Windows Service and authenticated, versioned IPC; privileged engine installation layout.
2. Upstream-socket route exclusion/interface binding and actual TUN/DNS in an isolated VM.
3. OS kill switch and recovery after core/service/UI crashes, sleep and network changes.
4. HTTPS subscriptions and Android backup compatibility.
5. Tray, installer/update policy; Linux service/storage after Windows boundaries stabilize.

## Not implemented / not claimed

Working system TUN, system-wide connection, kill switch, DNS/IPv6 leak protection,
background service, tray, HTTP subscription downloads, Android backup import,
automatic updates, production installer, Linux protected storage or full protocol parity.
HTTP loopback transfer and config acceptance do not establish remote-provider or
TLS/Reality/UDP interoperability. No host network settings were modified during tests.
