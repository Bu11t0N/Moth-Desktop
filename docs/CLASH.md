# Clash policy profiles

In **Servers → Clash profiles**, choose a YAML file and open the preview.
For a saved subscription, choose **Clash YAML** on its card. The preview
shows the nodes, groups, providers, rule order and DNS options. Save the
whole policy, or explicitly import compatible standalone nodes. Standalone
nodes do not retain groups, chaining, rules or DNS.

Choose **Connect** on the saved policy. It uses the selected Moth TUN or
HTTP/SOCKS mode. **Show active groups** displays the live selections and
lets the connection owner change a select group. Changes and automatic
measurements are for the active session. Updating an imported subscription
or provider replaces its saved copy only after all downloads and validation
succeed. Connect again to use a refreshed copy. If an update fails, the
last validated copy remains available.

## Supported policy

- Named SOCKS5, HTTP, VLESS, VMess AEAD, Trojan, Shadowsocks, Hysteria2,
  TUIC and AnyTLS nodes with checked fields shown in the preview.
- Select, url-test and fallback groups. A named proxy can use
  `dialer-proxy` for a chain. Legacy relay groups, unmanaged plugins and
  local provider files are incompatible.
- Inline and HTTPS proxy providers (YAML `proxies`) and rule providers
  (YAML `payload` or text). Rule behaviors are domain, ipcidr and classical
  with the supported rule types. Provider files are downloaded as the
  desktop user, validated, and passed as inline payloads. Provider redirects
  and provider-specified headers are unavailable. Subscription redirects,
  device header consent and VPN-only settings retain the Moth rules.
- Ordered DOMAIN, DOMAIN-SUFFIX, DOMAIN-KEYWORD, IP-CIDR, IP-CIDR6,
  RULE-SET, GEOIP, GEOSITE and final MATCH. GEOIP uses the pinned V2Fly GeoIP
  release 202611080027. GEOSITE uses V2Fly domain-list-community release
  20261118141523. A category absent from those datasets fails validation.
- DNS nameservers, numeric bootstrap, selected nameserver policies,
  redir-host and fake-IP. IP and HTTPS resolver forms are supported. DNS
  omitted in the profile uses Moth DNS and bootstrap. An explicit Clash DNS
  section conflicts with custom Moth DNS settings.

The preview blocks unrecognized fields, unsupported rule and provider
formats (including MRS), external control, TUN configuration, local file
references and scripting. Moth manages listen ports, LAN permissions and
the authenticated loopback controller. Documents and the resolved policy
each have a 192 KiB limit. The private local store holds up to eight policy
profiles. Compatibility depends on configuration, engine revision and geo
dataset. A preview can report compatibility before remote provider contents
are available. Saving performs the final pinned-engine validation.

## Capture and protections

For TUN, Moth sets MTU and IPv6 capture (rejecting IPv6 when disabled).
DNS is captured first, then app splitting and ordered Moth route overrides
apply to other traffic. The remaining flow reaches Mihomo, which evaluates
its first matching rule. The connection's verification request follows a
privately managed path so split-app settings cannot provide a false result.
Proxy mode uses the selected policy for applications that use its listener.
Local capture split and Moth route overrides require TUN.

The persistent egress guard rejects the entire Clash mode. Its fixed,
verified upstream exceptions do not cover a policy's dynamic provider
nodes, chains, DNS and explicit DIRECT exits. Moth never releases the guard
automatically. Policy-level DIRECT is intentional and can send traffic
outside the proxy. The native download channel is blocked while a Clash
policy with a possible DIRECT exit is active, so VPN-only refresh cannot
silently go direct. A profile blocking the configured HTTPS check cannot
be marked connected. Use an appropriate health-check URL for the policy.

The current standalone Moth reconnect/failover features apply to server
profiles. Use Clash fallback groups for policy failover. Existing
server, subscription, user storage and backup formats stay available.
