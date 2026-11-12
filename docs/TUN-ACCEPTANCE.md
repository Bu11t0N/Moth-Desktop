# Next milestone: Windows system tunnel acceptance

Do not execute these system-network tests on the developer's host. Use an isolated
Windows 11 x64 VM with a snapshot and a second clean user account. The M1b diagnostic
service and successful IPC/SCM tests are prerequisites, not proof of a working VPN.

## Before enabling a connect command

1. Identify the physical/default IPv4 and IPv6 interfaces before adding the tunnel.
   Determine how pinned Xray binds/excludes its upstream sockets on Windows; test it,
   rather than assuming sing-box auto-detect applies to the separate Xray process.
2. Confirm the Windows TUN driver requirements of the pinned sing-box binary. Any
   extra DLL/driver must have pinned hashes, notices and a protected installation path.
3. Add a typed connect/disconnect protocol, never raw JSON/routes/shell commands.
   Service revalidates the profile and owns authoritative lifecycle state.
4. Bind active connection ownership to the authenticated user's SID, never a claimed
   ID or one ephemeral pipe handle. Another account cannot replace/read its profile.
5. Specify bootstrap DNS, IPv6 behavior, LAN policy and failure policy explicitly.
   Do not advertise kill switch until OS filtering survives the defined crash cases.
6. Design a credential-free restoration journal and ownership of every OS change.
   Recovery removes only Moth-owned changes and preserves concurrent DHCP/user updates.

## VM fixture layout

- One NAT/default interface for the VM; a private test network hosts controlled Xray
  protocol servers and HTTP/HTTPS endpoints. Use synthetic credentials exclusively.
- Test certificates/CA are installed only in the VM's relevant trust store. Never
  disable TLS validation or install a test CA on the developer host.
- Capture baseline routes, adapter DNS and filtering state. Compare owned semantic
  changes on restore, not a byte-for-byte snapshot that would erase unrelated updates.
- Maintain an independent emergency way to restore the VM snapshot/network.

## Required evidence

| Scenario                                    | Required outcome                                                            |
| ------------------------------------------- | --------------------------------------------------------------------------- |
| Connect / disconnect / repeat               | Real traffic takes the selected server path; owned changes restore          |
| Upstream endpoint route                     | No routing loop after TUN default routes are active                         |
| Hostname endpoint + DNS                     | Bootstrap stays defined; application DNS follows the selected tunnel policy |
| IPv4 / IPv6 / dual stack                    | Matches the announced policy; no silent direct fallback                     |
| TCP / UDP                                   | Actual transfer verified independently of UI state                          |
| GUI close / reopen                          | Service state remains truthful and does not depend on a clicked button      |
| Core killed                                 | Child cleanup, explicit error/recovery state and configured traffic policy  |
| Service killed / restarted                  | No orphan cores, deterministic ownership/restoration, no false protection   |
| Sleep / resume, cable/Wi-Fi change          | Correct new upstream interface and bounded recovery                         |
| Failed config / missing driver / no network | Bounded failure without partial routes or DNS left behind                   |
| Second Windows account                      | No takeover or access to another user's credentials/session                 |
| Disconnect during startup                   | Cancellation does not resurrect a stale connection                          |
| Install / removal / reboot                  | Service and owned network changes follow the stated lifecycle               |

For each run record OS/core versions, commit, fixture, observed egress, DNS/route/filter
evidence and restore result. Redact credentials and subscription URLs. Core config
acceptance or a single HTTPS 204 result is not sufficient for these acceptance gates.
