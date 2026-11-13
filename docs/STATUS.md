# Status — 2026-11-13

Version 0.3.1 is in release validation.

## Implemented

- Previous profile import, DPAPI, pinned engines, HTTPS probes and authenticated SCM service.
- System TUN using sing-box with embedded Wintun; IPv4/IPv6 routes and DNS on the private adapter.
- Xray upstream interface binding, server resolution before routing changes, authenticated local bridge.
- Connection success requires HTTPS sourced from the TUN address. No direct/proxy fallback.
- IPC v2 typed connect/disconnect/status, authenticated SID ownership, startup cancellation and bounded startup.
- Service owns the connection independently of the GUI and monitors both engines every 500 ms.
- Connect/disconnect UI polls service state; service loss removes connected indication.
- Explicit service installation bootstrap, version 0.3.1 package and Desktop delivery planned.

## Verification

Local portable/configuration/IPC/UI checks and disposable Windows TUN/SCM CI are running.
Do not interpret the presence of implementation as completed network acceptance.
No host network settings were changed for development tests.

## Limitations

No persistent kill switch: engine/service failure tears down the tunnel and normal networking returns.
No automatic reconnect after sleep/network switch; disconnect and connect again.
DNS on physical adapters is not rewritten. LAN/more-specific existing routes can bypass TUN; applications with explicitly bound adapters are outside the current guarantee.
No claim of comprehensive IPv6/DNS leak protection or compatibility with concurrent VPNs.
Linux retains compiling common crates; daemon, storage and TUN integration remain pending.
Native GUI DPI/accessibility acceptance and production signing remain pending.
