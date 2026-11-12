# Status — 2026-11-12

Current milestone: M0, desktop foundation. No working VPN yet.

## Implemented

- Rust workspace: core / platform / Tauri boundaries.
- VLESS/Trojan URI and Base64 import, TCP/WS/gRPC/XHTTP, TLS/Reality profile model.
- Strict unsupported-option errors, deduplication, selection and removal.
- Windows DPAPI persistence with atomic replacement and credential-free snapshots.
- Russian desktop UI: home, servers, settings, import/delete dialogs.
- Structural Xray outbound generation; engine validation still pending.

## Verification

- Windows local `cargo test --workspace --locked`: 9 tests passed (8 domain + 1 DPAPI).
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- Frontend TypeScript + Vite production build: passed.
- Playwright: 3 scenarios passed, including native-adapter mocking, safe rendering,
  import/delete, keyboard cancellation and absence of external resource requests.
- `npm audit`: no known vulnerabilities in the resolved dependency set.
- Browser screenshot inspected at 1120px. Native WebView2 visual/runtime acceptance
  and additional DPI/screen-reader checks remain pending.
- Windows optimized EXE compiled with `npm run desktop:build -- --no-bundle`.
- Remote CI is configured for Windows build/tests and Linux domain/platform tests;
  its result must be checked on the published commit.

## Next

1. Pin and validate desktop sing-box/Xray against fixture matrix.
2. Add Windows Service and authenticated IPC.
3. Prove TUN/DNS in an isolated VM, then filtering/kill switch and recovery.
4. HTTPS subscriptions and Android backup compatibility.
5. Tray, signed installer/update policy; Linux implementation after Windows boundaries stabilize.

## Not implemented / not claimed

TUN, connection, kill switch, DNS leak protection, latency checks, background service,
tray, HTTP subscription downloads, backup import, automatic updates, production installer,
Linux protected storage, English localization, real-engine compatibility tests.
