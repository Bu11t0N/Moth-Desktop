# Windows service — 0.3.1

GUI (ordinary user) -> authenticated named pipe -> LocalSystem service -> pinned engines.
The service owns the connection, independently of the GUI window. No connection is restored automatically after a service restart.

## Installation

Extract the whole Windows package and run Install-Service.cmd. The bootstrap requests Windows UAC and calls Service.ps1 using the built-in Windows PowerShell 5.1.
It creates a protected fixed installation at %ProgramFiles%/Moth Desktop, copies only the service and two pinned cores (plus notices), validates ACLs, registers an automatic-start own-process LocalSystem service and verifies its health. Installation alone does not start a VPN.
Existing directories/services are refused; in-place upgrade is not yet implemented. Remove an older service explicitly and review/move its preserved installation directory before a new installation.

Manual commands in administrator PowerShell, from the extracted package:

~~~powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Service.ps1 -Action Install -PackageDirectory .
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Service.ps1 -Action Status
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Service.ps1 -Action Remove
~~~

Removal stops/unregisters the service and preserves its files. GUI profiles stay in the current user's DPAPI store.

## Authentication and installation boundary

Service name: MothDesktopService. Stable endpoint: \\.\pipe\Moth.Desktop.Service.v1; the endpoint name is stable while the message protocol is version 2.
The client queries SCM for RUNNING PID, own-process type and LocalSystem account, opens the pipe with identification-only SQOS and explicit data rights, verifies the actual server PID and rechecks SCM before writing credentials. It needs no SeDebugPrivilege.

The first server pipe uses FIRST_PIPE_INSTANCE; every instance rejects remote clients. Its DACL grants SYSTEM/admin full access, interactive users data read/write, read attributes and synchronization, excluding FILE_CREATE_PIPE_INSTANCE.
After a bounded read, synchronous pipe impersonation obtains the actual caller SID. Low-integrity and AppContainer clients are rejected. Interactive/admin membership is required. RevertToSelf must succeed before any await or engine execution. No caller-supplied SID is trusted.

Production requires the protected Program Files location, SYSTEM/Administrators ownership and no untrusted write ACLs on root, engines, service and core binaries. Reparse points are rejected. Both engine hashes are verified against compile-time pins at each launch.

## IPC v2

A 4-byte little-endian length precedes a JSON envelope (1..32768 bytes). Unknown fields/commands, invalid versions, malformed JSON and wrong response correlation are rejected. Frames have bounded read/write times; clients wait up to 30 seconds for a response.
Commands: status, probe {link}, connect {link}, disconnect, tunnelStatus. No arbitrary executable, path, raw core JSON, check URL, OS command or service stop action is accepted. The service strictly re-imports exactly one link. Credential-bearing commands/buffers are zeroized at the owned IPC boundary and never implement Debug.

One system connection is owned by the authenticated Windows SID, across pipe requests and GUI restarts. A different account can see the phase but cannot disconnect or replace it. A connecting operation can be cancelled by its owner. Status returns phase/ownership/error code, never a profile, endpoint or credential.
The service accepts at most eight active requests and one probe at a time. Connection startup is bounded to 25 seconds. Successful startup requires an HTTPS 204 response sourced from the TUN address. A 500 ms monitor checks both processes; failure tears down the remaining engine and publishes failed state.

## Networking/lifecycle limits

sing-box owns the private Wintun adapter, its IPv4/IPv6 routes, DNS and dynamic WFP session. Physical adapter DNS/routes are never overwritten. Xray sockets bind to the upstream interface discovered before TUN activation, independently of sing-box. The local bridge explicitly binds loopback.
Job Object termination and kill-on-drop contain both children on cancellation/service exit. This teardown is fail-open: there is no persistent OS kill switch. No comprehensive DNS/IPv6 leak guarantee, automatic sleep/network recovery or simultaneous-VPN compatibility is claimed. More-specific pre-existing routes/LAN and explicitly bound applications can bypass TUN.

## Development and evidence

--console uses an explicit development pipe and same-user/PID validation. It refuses LocalSystem and system connect/disconnect commands. The GUI never falls back to it. Local tests never modify networking.
--status and --tunnel-status are authenticated production diagnostics. --connect reads a bounded link from stdin (never command-line arguments); --disconnect uses the same SID ownership rules.
Disposable CI VM tests: scripts/test-tun.ps1 and scripts/test-service-scm.ps1, including test-service-tunnel.ps1. See STATUS.md for actual results; configured tests are not evidence of passing acceptance.

## GUI 0.3.2 tray

The main-window close button hides to tray and preserves the running GUI and service connection. Tray Open/left-click or launching the shortcut restores the same instance. Tray Disconnect stops only the caller-owned tunnel. Exit blocks new Connect requests and confirms disconnection before ending the GUI; a pipe error retains the application unless SCM independently confirms the service is stopped/absent. The LocalSystem service itself stays installed/running for the next GUI launch. Existing 0.3.1 services support these IPC v2 commands without an upgrade.
