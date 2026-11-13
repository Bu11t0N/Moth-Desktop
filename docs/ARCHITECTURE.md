# Architecture decisions

## ADR-001: one desktop repository, isolated from Android

One Cargo workspace and desktop release stream. Android is a behavior/format reference;
no submodule or write dependency on the Android tree. Initial preview version is 0.1.0,
independent of Android 1.0.2. All new code GPL-3.0-or-later.

## ADR-002: replaceable UI

`moth-core` owns profiles and format/config logic; no OS or Tauri dependency.
`moth-platform` owns OS services, persistence and capability reporting.
Tauri is an adapter with explicit commands. TypeScript only renders credential-free snapshots.
Iced is a potential separate adapter after MVP, not a simultaneous deliverable.

## ADR-003: privilege separation (M1)

GUI (normal user) -> authenticated local IPC -> Rust system service -> managed engine processes.
Windows transport: named pipe with explicit ACL and peer validation; Linux: Unix socket with
peer credentials (or D-Bus/polkit; final choice in Linux ADR). The service revalidates inputs,
does not accept arbitrary paths, commands or caller-provided raw core JSON. No public HTTP API.
Service owns real connection state; GUI never infers it from a clicked button or running PID.

M1b implements the Windows service and protocol v1 for health/probes. There is no system
tunnel command yet. See SERVICE.md for peer authentication, installation ACLs, the isolated
development harness and explicit acceptance limits. Future incompatible schemas require
a protocol version change; service health never implies system-wide protection.

## ADR-004: engine strategy

Prefer pinned separate sing-box/Xray processes over linking Android libbox. Xray-only profiles
may need a sing-box TUN bridge; topology is to be proven in M1, not assumed complete.
Review Android patches individually (socket protection, shutdown, dependency conflicts).
Do not blindly transplant Android VPN socket controls into Windows.

M1a implements pinned external engines in `moth-engine`, native config checks and
short-lived authenticated local SOCKS for HTTPS probing. sing-box TUN / Xray bridge
configs validate but are not executed as a system VPN. See ENGINES.md for evidence,
Android patch review, process containment and explicit remaining limitations.

## ADR-005: protected storage

Windows DPAPI, current-user scope. Serialized library encrypted before any file write,
same-directory atomic replacement. No cleartext backup/temporary JSON. UI never receives
passwords, UUID credentials, raw URI or generated JSON. Imported text is discarded when
dialog closes. No Debug implementation on credential-bearing models. DPAPI plaintext
buffers are cleared where owned by the encryption boundary; full heap zeroization is not
claimed (models/serialization clones exist).

DPAPI doesn't protect against malware running as the same user. Linux persistence is
explicitly unsupported pending a Secret Service/keyring design. No silent plaintext fallback.
Corruption is a startup error, never an empty library implicitly saved over the original.

## ADR-006: local UI content only

No remote scripts/fonts/images. CSP limits scripts/connect sources; no shell/fs plugin.
Imported strings are escaped before rendering. A web browser can view the interface but
cannot import, save or claim protection. Future HTTPS subscription downloading stays in
Rust, never through the webview.

## Reference documentation

- https://v2.tauri.app/concept/architecture/
- https://v2.tauri.app/security/capabilities/
- https://sing-box.sagernet.org/configuration/inbound/tun/
- https://sing-box.sagernet.org/configuration/route/rule/
- https://learn.microsoft.com/en-us/windows/win32/fwp/about-windows-filtering-platform
- Android source reference: https://github.com/Bu11t0N/Moth/tree/36dfa07da6b813493863ac48eadf4a0e67798ad1

## ADR-007: Windows TUN in 0.3.1

The service owns sing-box TUN and a credential-authenticated loopback VLESS bridge to Xray.
Before activation it resolves the server and uses GetBestInterfaceEx / ConvertInterfaceLuidToAlias.
Xray sockopt.interface binds sockets to that upstream interface (IP_UNICAST_IF / IPV6_UNICAST_IF).
The separate Xray process is not protected by sing-box auto_detect_interface alone.
DNS/routes belong only to the private Wintun adapter; physical adapter DNS/routes are never overwritten.
Windows job teardown removes the adapter and dynamic WFP session; this is fail-open, not a kill switch.
Protocol v2 adds SID-owned connection lifecycle; the stable pipe endpoint remains unchanged.
Normal console development mode refuses system connect/disconnect. Real TUN acceptance requires a disposable CI Windows VM.

Sources reviewed at pinned versions: [Xray Windows socket binding](https://github.com/XTLS/Xray-core/blob/1b1647cd9ddf0abc34b981d4611fc03bcac1ca74/transport/internet/sockopt_windows.go), [sing-tun adapter lifecycle](https://github.com/SagerNet/sing-tun/blob/v0.7.13/tun_windows.go), [embedded Wintun loader](https://github.com/SagerNet/sing-tun/blob/v0.7.13/internal/wintun/dll_windows.go).

## ADR-008: desktop tray lifecycle in 0.3.2

The Tauri adapter owns a native tray and the official single-instance plugin. CloseRequested hides the retained main webview without invoking IPC or terminating the app. Tray left click, Open and a second process restore the existing window. A Rust timer polls authenticated service state even when WebView timers are throttled.

Disconnect and Exit share the same bounded stop path. It blocks new GUI Connect commands, cancels an already accepted connect, waits for outstanding Connect leases, then confirms disconnection again. Exit is authorized only after this barrier succeeds. A missing pipe is insufficient evidence: SCM must confirm that the service is absent/stopped before an unavailable service is treated as safe to exit. Other-account connections are never stopped. Errors retain the tray/process and restore the window with a user-visible message. No protocol change is needed; service 0.3.1 remains supported.

Debug-only disposable Windows CI acceptance exercises actual native close events with live TUN, a real second process, and the same tray menu dispatch for Exit. Its driver is excluded by debug_assertions from release binaries. This does not simulate protection and does not run network tests on the developer host.
