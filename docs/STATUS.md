# Status — 2026-11-13

Version 0.3.1: Windows system TUN development release.

## Implemented

- Previous profile import, DPAPI, pinned engines, HTTPS probes and authenticated SCM service.
- System TUN using sing-box with embedded Wintun; IPv4/IPv6 routes and DNS on the private adapter.
- Xray upstream interface binding, server resolution before routing changes, authenticated local bridge.
- Connection success requires HTTPS sourced from the TUN address. No direct/proxy fallback.
- IPC v2 typed connect/disconnect/status, authenticated SID ownership, startup cancellation and bounded startup.
- Service owns the connection independently of the GUI and monitors both engines every 500 ms.
- Connect/disconnect UI polls service state; service loss removes connected indication.
- Explicit service installation bootstrap, versioned 0.3.1 ZIP and verified extraction to Desktop/Moth/0.3.1 with a launch shortcut.

## Verification

- Implementation adc4f2c: Windows and Linux CI passed: https://github.com/Bu11t0N/Moth-Desktop/actions/runs/43434893973
- Disposable Windows VM: real TCP/UDP over IPv4/IPv6 through TUN, disconnect/reconnect, teardown and preservation of physical adapter DNS/routes.
- Installed SCM service: separate non-administrator account connects through the real HTTPS verifier; explicit DNS query to the TUN gateway succeeds.
- SID isolation rejects another account's disconnect. Reconnect, forced Xray termination, failure reporting, recovery and service shutdown/restart pass.
- Native config matrix, loopback credential failure, DPAPI, actual pipe spoof/version/frame handling, ownership unit test, formatting and Clippy pass.
- Seven Playwright scenarios and TypeScript/Vite build pass, including cancellation and loss of service. These are browser adapter tests, not native WebView2/DPI acceptance.
- Windows release EXE version 0.3.1 verified. ZIP extracted to Desktop/Moth/0.3.1; every file checksum and shortcut target verified.
- Final ZIP SHA-256: 3c9155623b46ab42533eb8d2fe070986bb1d0cdbc185f162c12d207639460816.
No host network settings were changed for development tests.

## Limitations

No persistent kill switch: engine/service failure tears down the tunnel and normal networking returns.
No automatic reconnect after sleep/network switch; disconnect and connect again.
DNS on physical adapters is not rewritten. LAN/more-specific existing routes can bypass TUN; applications with explicitly bound adapters are outside the current guarantee.
No claim of comprehensive IPv6/DNS leak protection or compatibility with concurrent VPNs.
Linux retains compiling common crates; daemon, storage and TUN integration remain pending.
Native GUI DPI/accessibility acceptance and production signing remain pending.
