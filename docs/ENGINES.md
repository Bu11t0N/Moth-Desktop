# Desktop engines — M1a, preview 0.2.0

## Pinned upstream binaries and corresponding sources

- Xray 26.10.21: official XTLS/Xray-core Windows/Linux x64 release.
  Source revision: `1b1647cd9ddf0abc34b981d4611fc03bcac1ca74`.
  Full source: https://github.com/XTLS/Xray-core/tree/1b1647cd9ddf0abc34b981d4611fc03bcac1ca74
  Source archive: https://github.com/XTLS/Xray-core/archive/1b1647cd9ddf0abc34b981d4611fc03bcac1ca74.tar.gz
  Upstream build definitions: `.github/workflows` and `go.mod`/`go.sum` in that revision.
- sing-box 1.12.25: official SagerNet/sing-box Windows/Linux x64 release.
  Source revision: `73bfb99ebce7923c485435e4faf8571b412065a9`.
  Full source: https://github.com/SagerNet/sing-box/tree/73bfb99ebce7923c485435e4faf8571b412065a9
  Source archive: https://github.com/SagerNet/sing-box/archive/73bfb99ebce7923c485435e4faf8571b412065a9.tar.gz
  Upstream build definitions: `.github/workflows`, `release` and `go.mod`/`go.sum` in that revision.

No source patches or custom engine builds in M1a. Exact archive and executable SHA-256
pins are checked into `engines.lock.json`. The fetch script verifies the archive before
extracting only the named executable, then verifies the executable independently.
Runtime checks the executable against a manifest compiled into the Moth binary;
it does not trust a writable external checksum file. Downloads occur only via the
explicit development fetch command/CI, never on application startup.

## Runtime behavior

`moth-engine` has no Tauri dependency and never changes system proxy, routes, DNS or
firewall. It launches Xray with an authenticated SOCKS listener on 127.0.0.1, selects
a profile internally and issues a fixed HTTPS 204 request. No arbitrary path, command,
configuration or check URL can be submitted by the frontend. Port allocation has
a brief bind-to-launch race; random proxy credentials plus process-exit checks prevent
successful checks through an unrelated unauthenticated listener.

Configuration travels on stdin; EOF is explicit. Raw core stdout/stderr is discarded.
Only OS-essential environment variables are inherited, preventing Xray config-directory
overrides from another installation. HTTPS verification uses rustls with its trust roots;
custom user CA support is not yet implemented for the probe request itself. Remote server
TLS verification remains enabled in Xray. No insecure certificate option is imported.

Config write, startup and requests have bounded timeouts. Normal completion/error
reaps the engine. Tokio kill-on-drop handles cancellation; Windows additionally uses
a non-inheritable Job Object with KILL_ON_JOB_CLOSE. The Windows job provides OS-level
child cleanup if the owning GUI process exits. This is process containment, not kill switch.
Linux cancellation has Tokio process cleanup; abnormal parent-exit containment remains
a Linux service responsibility.

## Evidence and limits

Native config matrix: VLESS/Trojan over TCP, WS, gRPC and XHTTP; VLESS Reality/Vision;
unencrypted VLESS only to a loopback/private destination. Tests call `xray run -test`
and `sing-box check`. A real loopback VLESS client/server carries an HTTP 204 request,
then rejects a wrong UUID. This proves that path, not all transports' interoperability.
No remote provider, TLS/Reality handshake, UDP or DNS-leak acceptance is claimed yet.

TUN planning: sing-box -> UUID-authenticated local VLESS -> Xray -> selected server.
DNS route uses the bridge and a remote DoH resolver. The configuration validates, but
is NOT executed: binding/excluding upstream sockets, interface discovery, network recovery,
privileged service IPC and firewall policy remain M1b/M2 work. `strict_route` alone is
not a system kill switch. There is deliberately no sing-box run path in this crate yet.

## Android patch review

- Android `VpnService.protect(fd)` controls are Android-specific; do not transplant.
- The patched sing-tun close/dispatcher path must be re-evaluated with actual desktop
  TUN shutdown tests. Successful `check` is not sufficient evidence.
- Go module/qpack compatibility patches used to combine cores in one Android library
  are unnecessary for separate desktop processes.
- Local Xray test servers require explicit permission for private destinations in the
  freedom outbound's finalRules. Only the isolated test server enables this permission.

## Next gate

Windows Service and authenticated pipe, protected installation directory, versioned
GUI/service protocol, private runtime config lifecycle, then TUN/DNS tests in an isolated VM.
Hash-then-execute in a user-writable development folder is not a defense against malware
running as the same user; privileged service engines must live in an administrator-owned location.
