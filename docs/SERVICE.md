# Windows service and IPC — M1b control plane, preview 0.3.0

This milestone implements a real Windows Service and a local control channel.
It does not run a system TUN, change DNS/routes/firewall, implement kill switch,
or claim that a positive service health response protects traffic.

## Components

- `moth-ipc`: Tauri-independent versioned protocol, framed transport and Windows identities.
- `moth-service`: SCM dispatcher, stop/shutdown handling, bounded request server and probe executor.
- Tauri: authenticates the production service, uses it for server probes when available,
  and exposes a separate service health action in Settings.
- Existing local probe remains available without installation in this development preview.
  An authentication/access/version/protocol error never triggers a local fallback.

## Production trust boundaries

Service name: `MothDesktopService`; account: LocalSystem; startup: manual in this preview.
Fixed pipe: `\\.\pipe\Moth.Desktop.Service.v1`.

The client queries Windows SCM for the registered RUNNING PID, opens the local pipe with
identification-only SQOS and explicit read/write-data rights, checks the actual server PID,
checks its process token is LocalSystem, and rechecks SCM. Profile serialization/writing to
the pipe happens only after peer verification. A squatted endpoint cannot receive credentials.

The server requires LocalSystem, a protected fixed Program Files installation and verified
engine hashes. The first pipe uses FIRST_PIPE_INSTANCE, every instance rejects remote
connections. Its explicit DACL grants SYSTEM/admin full access and interactive users only
read/write-data, read attributes and synchronization. It deliberately omits generic write
and FILE_CREATE_PIPE_INSTANCE for interactive clients. No Everyone/Anonymous access.

After reading a bounded frame, the server identifies the actual pipe client's token in a
synchronous impersonation scope. AppContainers and low-integrity callers are rejected;
interactive users or administrators are allowed. RevertToSelf must succeed before any
engine work or await; failure aborts the service process. No caller-supplied SID is trusted.
Requests do not expose another user's profiles: each probe carries its own validated link;
the service has no persistent shared profile store or connection state in this milestone.

The service requires `%ProgramFiles%\Moth Desktop`, rejects reparse points and untrusted
ownership/write ACLs on the installation root, engines directory, service and core binaries.
Only SYSTEM/Administrators may own or write these objects. Install refuses an existing
directory/service instead of silently taking it over. Production is not launched from a
user-writable source checkout. Administrator compromise is outside this boundary.

## Protocol v1

Each direction uses a 4-byte little-endian length and one JSON envelope (1..32768 bytes).
Read/write have timeouts. Unknown fields/commands, malformed JSON, oversized/empty frames,
wrong response correlation IDs and incompatible versions are rejected. Each request has a
UUID correlation ID. Probe requests contain secrets and do not implement Debug; buffers
and the owned link are cleared on drop where owned by the transport boundary.

Supported commands: `status {}` and `probe {link}`. There is no arbitrary command, core path,
raw engine JSON, URL-to-check, service stop command or system networking operation in IPC.
The service re-imports the link using the same strict parser, requires exactly one profile,
then invokes the pinned engine. Replies contain status metadata, measured latency or a
closed set of error codes. Core output and credentials are not logged or echoed.
At most 8 requests are active, 1 probe runs at a time, frames have a 2s deadline and a probe
has a 25s execution deadline. Shutdown aborts active tasks, dropping owned engine jobs.

## Development console

`moth-service --console` is an ordinary-user harness with an explicitly different pipe name,
`\\.\pipe\Moth.Desktop.Dev.<PID>`. It refuses LocalSystem. Development clients require the
explicit expected PID and the same Windows user SID. The normal GUI never discovers or
falls back to this endpoint. It permits non-privileged IPC integration tests on the host.
No service installation or network setting change occurs in those tests.

## Explicit manual install/remove (development builds)

Unsigned development packages must be reviewed before administrator installation.
Extract the entire package; PowerShell 7 is required. No GUI action installs the service.
In an elevated PowerShell 7 window, from the extracted directory:

```powershell
./Service.ps1 -Action Install -PackageDirectory .
```

The script creates the protected directory/ACLs, copies service/engines/notices, verifies the
security policy, registers a manual-start LocalSystem service, starts it and authenticates
its health response. It never changes system network settings. It refuses in-place upgrades.

```powershell
./Service.ps1 -Action Status
./Service.ps1 -Action Remove
```

Removal stops and unregisters this service, preserving installation files for review.
Do not infer VPN protection from RUNNING or a successful health response.

## Evidence and remaining acceptance

Host tests: real console process/pipe health, strict version/frame handling, recovery after
bad requests, first-instance protection and rejection of a fake server before a secret is
sent. Core share-link roundtrip covers reserved credentials and IPv6. UI test keeps service
health distinct from VPN status. Windows SCM install/start/health/stop/restart/removal is
configured on a disposable GitHub-hosted runner only (`scripts/test-service-scm.ps1`).

Separate non-administrator/multiple-logon-session acceptance, low-integrity/AppContainer
negative tests, independent security review and mid-probe service-crash cleanup remain
required before production release. Console/administrator CI tests do not establish them.
Linux uses the portable protocol crate but has no implemented daemon/transport yet.

Next: interface discovery/upstream route binding, then actual TUN/DNS in an isolated VM.
Per-user ownership and reconnect/restore policy must be added when persistent tunnels exist.

## References

- https://learn.microsoft.com/en-us/windows/win32/ipc/named-pipe-security-and-access-rights
- https://learn.microsoft.com/en-us/windows/win32/api/namedpipeapi/nf-namedpipeapi-impersonatenamedpipeclient
- https://learn.microsoft.com/en-us/windows/win32/ipc/pipe-functions
- https://docs.rs/windows-service/0.8.1/windows_service/
