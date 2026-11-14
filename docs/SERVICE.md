# Windows service — 0.3.3

GUI (ordinary user) -> authenticated named pipe -> LocalSystem service -> pinned engines.
The service owns the connection, independently of the GUI window. No connection is restored automatically after a service restart.

## Installation

### Graphical installer candidate

The NSIS development installer requests elevation once, installs the GUI, LocalSystem service and pinned cores together in the fixed protected Program Files directory, creates all-user Start/Desktop shortcuts and a Windows uninstall entry. It does not launch an elevated GUI. Start the application from its shortcut after installation; ordinary use requires no elevation. Disconnect VPN and choose tray Exit before upgrading/removing. A running installed GUI or active service tunnel blocks replacement. Windows account-bound profiles remain in the same per-user DPAPI location, including profiles created by portable 0.3.1/0.3.2; the administrator installer never decrypts or copies them.

Use the Windows uninstall entry to remove a graphical installation. It stops/unregisters the service and removes only unchanged files recorded in the installation manifest. Per-user data, unknown/modified files and previous installation backups are preserved. The manual service bootstrap refuses to replace a graphical installation with a service-only payload. An administrator who changes/removes the protected maintenance script or manifest may need to repair using the installer before uninstalling.

The graphical installer uses the same staging/health/rollback transaction described below, expanded to GUI and uninstaller files. It is not an automatic updater. SHA-256 integrity checks do not replace publisher authentication; Authenticode and signed in-app updates remain pending. A WebView2 runtime is currently a prerequisite for the GUI.

Reproducible build: create a sanitized portable package using `build-windows.ps1` and `package-windows.ps1`, then call `build-installer-windows.ps1 -PackageDirectory <extracted-package> -OutputDirectory <new-output>` with NSIS 3.11. `test-installer-windows.ps1` executes the resulting installer/uninstaller in a disposable Windows VM, including both legacy upgrades, forced final-health rollback of running/stopped installations, retry, shortcuts and DPAPI/unknown-file retention. Fault injection happens only in a separate test-source build. See STATUS.md for actual acceptance results.

### Portable package and manual service bootstrap

Extract the whole Windows package and run Install-Service.cmd. The bootstrap requests Windows UAC and calls Service.ps1 using the built-in Windows PowerShell 5.1.
It creates a protected fixed installation at %ProgramFiles%/Moth Desktop, copies only the service and two pinned cores (plus notices), validates ACLs, registers an automatic-start own-process LocalSystem service and verifies its health. Installation alone does not start a VPN.
For an existing 0.3.1/0.3.2/0.3.3 service, disconnect VPN first, extract the new package separately and run its Install-Service.cmd. Do not overwrite a running GUI or delete the installation directory. Close the old GUI using tray Exit, then open the new GUI after installation.

The installer authenticates the existing service with the new IPC v2 client, checks the fixed SCM executable path, LocalSystem account and existing production ACLs, rejects active tunnels and unknown service versions, and stages a complete service/core set in an administrator-owned directory under Program Files. It verifies pinned engine hashes before stopping the service. Only then does it stop the service, rename the previous directory, promote the staged set, validate production ACLs, start the candidate and check authenticated version/protocol/core health. A failure restores the previous directory and its previous running/stopped state. User DPAPI profiles are outside these directories and are never copied, decrypted or migrated by the installer.

Previous installations remain in administrator-controlled `Moth Desktop.rollback-*` directories; failed candidates remain in `Moth Desktop.stage-*.failed`. These are retained for inspection rather than recursively deleted. Reinstallation after Remove reuses the preserved installation without manual cleanup. Manual package installation requires trusting the downloaded package; checksums are integrity checks, not publisher signatures. Automatic/signed updates, downgrade protection against a malicious administrator, and crash/power-loss recovery during directory promotion are not implemented. An unsuccessful automatic recovery produces an explicit error and preserves previous files for administrator review.

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
--status and --tunnel-status are authenticated production diagnostics. Status rechecks the pinned engine hashes; protocol availability is separate from an active VPN. --connect reads a bounded link from stdin (never command-line arguments); --disconnect uses the same SID ownership rules.
Disposable CI VM tests: scripts/test-tun.ps1 and scripts/test-service-scm.ps1, including test-service-tunnel.ps1. Upgrade acceptance: `scripts/test-service-upgrade.ps1` takes fully extracted candidate, 0.3.1 and 0.3.2 packages. It requires a disposable Windows CI runner or explicit `-DisposableVm` plus VMware hardware detection. It tests modified-core and unsafe-ACL refusal, post-start failure rollback, repeated/stopped updates, removal/reinstallation and byte-identical decryptable DPAPI preservation. It refuses an existing installation/profile. Failure is injected into a test-only copy of the installer, with no production fault-injection setting.

The service/network scripts also accept `-DisposableVm` after VMware hardware detection; the native GUI lifecycle is tested under a standard user. Windows release binaries are built through `scripts/build-windows.ps1`, which remaps compiler source paths and checks the resulting EXEs.

See STATUS.md for actual results; configured tests are not evidence of passing acceptance.

## GUI 0.3.2 tray

The main-window close button hides to tray and preserves the running GUI and service connection. Tray Open/left-click or launching the shortcut restores the same instance. Tray Disconnect stops only the caller-owned tunnel. Exit blocks new Connect requests and confirms disconnection before ending the GUI; a pipe error retains the application unless SCM independently confirms the service is stopped/absent. The LocalSystem service itself stays installed/running for the next GUI launch. Existing 0.3.1 services support these IPC v2 commands without an upgrade.
