# Desktop engines — 0.3.5

The exact upstream pins below are unchanged. The service selects Xray for the
existing six profile families and sing-box for Hysteria2/salamander, TUIC v5 and
AnyTLS. Both use a credential-authenticated loopback VLESS bridge from the TUN
or mixed listener. Numeric upstream pinning preserves TLS SNI and transport host;
Windows guard exceptions also identify the selected installed executable and
LocalSystem token. No custom protocol/cryptographic implementation is added.

New families support typed URI and strict sing-box JSON/Clash server subsets.
Port hopping, arbitrary raw engine options, WireGuard/AmneziaWG/SSH/ShadowTLS and
full Clash/Mihomo policy documents are not supported. Import rejects incompatible
options. Local pinned-engine checks and real loopback TLS/password/name tests
cover all three new families; this is not paid-provider interoperability evidence.

The optional mixed HTTP/SOCKS listener always authenticates. LAN binds an explicit
private interface address and requires source-IP allowlists; the controller and
private native download channel remain on loopback with separate secrets.
Proxy-only mode creates no TUN and never reports system-wide VPN protection.
Engine counters describe routed application traffic, not billing/wire totals.

The optional persistent guard is separate from engine teardown; without it the
historical fail-open behavior below remains. See STATUS.md for native acceptance.

## Earlier engine foundation

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

Profiles/configuration are private Rust values. No engine path, raw JSON or arbitrary check URL is accepted from the frontend. Config travels through stdin, core output is discarded and only OS-essential environment is inherited. Every launch rechecks the executable SHA-256 pin compiled into Moth.

Probes are short-lived authenticated SOCKS5h -> Xray -> fixed HTTPS 204, with rustls certificate verification and no direct/redirect fallback. They do not change system routing.

Service connections execute sing-box TUN (gVisor stack) -> UUID-authenticated local VLESS -> Xray -> selected server. The bridge explicitly binds 127.0.0.1; Xray binds the upstream physical interface. The server hostname is resolved before activation, numeric address is pinned for that connection, TLS/Reality SNI and transport host are preserved. DNS uses DoH to 1.1.1.1 through the bridge. Successful startup requires an HTTPS request sourced from 172.19.0.1, not a process-start indication.

The official sing-box executable embeds Wintun via sing-tun v0.7.13. No external writable wintun.dll is loaded or shipped separately. The upstream embedded loader and adapter lifecycle are linked in ARCHITECTURE.md.

Config write, validation, process startup and requests have bounded timeouts. Tokio kill-on-drop plus Windows KILL_ON_JOB_CLOSE jobs own engines. Private adapter routes/DNS and dynamic WFP filters disappear with teardown. This is process containment and fail-open cleanup, not a persistent kill switch.

## Verification and remaining limits

The native configuration matrix covers VLESS/Trojan TCP, WS, gRPC, XHTTP, Reality/Vision, private-only unencrypted VLESS. Loopback tests carry HTTP 204 and reject wrong credentials, including an interface-bound authenticated bridge. Disposable Windows VM tests exercise actual TUN routing and service lifecycle; exact passing evidence is recorded in STATUS.md after completion. No all-provider/transport interoperability or comprehensive DNS/IPv6 leak guarantee is implied.

## Android patch review

Android VpnService.protect is not transplanted to desktop. Separate upstream core processes avoid the Android combined-libbox qpack conflicts. Desktop shutdown is verified separately rather than assuming Android patches apply. Only isolated test server freedom outbounds permit private destinations. No engine source patches are used.

Production signing, full dependency/license audit, network/sleep recovery, kill switch, native GUI acceptance and Linux service containment remain later gates. Hash-then-execute in a writable development directory does not defend against same-user malware; privileged production engines use the protected installation.
