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

DPAPI doesn't protect against malware running as the same user. Other platforms
require an implemented protected backend; there is no silent plaintext fallback.
Corruption is a startup error, never an empty library implicitly saved over the original.

Linux foundation in the expanded 0.3.3 candidate uses the desktop Secret Service
default persistent collection with an encrypted DH session. One versioned secret
contains the library; the application writes no plaintext profile file and has no
fallback when the provider is unavailable/locked. The supported desktop must have
a password-protected keyring. The provider owns encryption at rest. Ambiguous or
corrupt items fail without silently replacing the library. Windows DPAPI and its
per-user path remain unchanged. Runtime/keyring acceptance is tracked in STATUS.md.

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


## ADR-009: diagnostics and manual service upgrades in 0.3.3

IPC v2 remains compatible with service 0.3.1. Build capabilities describe implementation;
runtime availability comes from authenticated service health and verified core hashes.
Connection state remains independent: a healthy service never implies connected VPN.
The installer accepts fixed installation paths only, validates existing SCM/ACL policy,
stages the service and pinned cores together, and verifies the started candidate before
success. Failure restores the prior files and service state; profiles remain in the user
DPAPI store. A trusted manually downloaded ZIP is still required. Signed automatic updates
and crash-consistent recovery are future work. Release compiler paths are remapped and
EXEs are scanned; local build paths and debug data are excluded from release packages.

## ADR-010: Linux transport foundation

The Linux client uses only `/run/moth-desktop/control.sock`, checks root-controlled
parents and a root-owned socket, then verifies SO_PEERCRED UID 0 before writing a
request. The daemon derives ownership from the kernel peer UID, never request data.
Root maintenance and desktop UIDs 1000–65533 with a private login runtime directory
are eligible. Fixed `/usr/lib/moth-desktop` installation files must be root-owned,
not group/other-writable and contain no symlinks; core hashes remain independently
verified. The bounded IPC v2 command set is unchanged. This transport foundation
does not by itself implement a Linux daemon or TUN.

References: [Tokio Unix peer credentials](https://docs.rs/tokio/latest/tokio/net/unix/struct.UCred.html),
[Secret Service blocking boundary](https://docs.rs/secret-service/5.2.0/secret_service/blocking/index.html).

## ADR-011: 0.3.5 owned network policy and observable modes

Persistent guard intent is root-protected, bounded and contains only owner and
cached numeric upstream endpoints/selected engine kinds. WFP provider/sublayer
and nftables table are fixed and owned; engine/session teardown does not release
them. Startup restores intent, but UI protection requires a live OS rule check.
Direct/per-app bypass and LAN listeners conflict with the guard.

New typed IPC commands distinguish TUN from proxy-only sessions without changing
legacy IPC v2 states. A legacy client sees an unsupported failed state for active
proxy mode, never a connected system VPN. Public mixed listeners authenticate;
LAN adds explicit bind/source restrictions. Native download routes use a separate
authenticated force-proxy listener, with protected user intent preventing direct
fallback across a proxy-only service restart. OS proxy recovery and login entries
belong to the ordinary user and preserve foreign modifications.

The secret-authenticated loopback Clash API is read only through native sampling.
Bounded summaries expose actual counters and connection fields; no API bearer,
raw config or raw logs enter webview snapshots. History is optional and bounded;
diagnostic event entry points accept only fixed typed codes. Export is previewed.
