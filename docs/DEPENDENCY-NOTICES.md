# Dependency notices

`python scripts/license-notices.py` checks the Rust/JavaScript source locks,
generator configuration, notice hashes and pinned-engine notice archive. Package
resources include the complete `licenses` directory. The archive is an ordinary
gzip-compressed tar file containing only regular files; extract it to read notices
or inspect SOURCE-INDEX.json. It is never executed by Moth.

Rust notices use cargo-about 0.9.3 with the checked-in configuration/template.
After `npm ci`, regenerate with:

```sh
python scripts/license-notices.py generate /path/to/cargo-about
```

Engine collection uses Go 1.27+ only as a tool for reading binary/module metadata
and running google/go-licenses v2.0.1. It never builds or patches Xray/sing-box.
Fetch all four pinned binaries with `node scripts/fetch-engines.mjs --all`.
Extract the two [exact upstream revisions](ENGINES.md) into one working directory,
preserving their default archive directory names. Install the notice tool into
a separate build-tool directory with `go install github.com/google/go-licenses/v2@v2.0.1`.
Set `CGO_ENABLED=0`, `GOARCH=amd64`, `GOFLAGS=-mod=readonly` and run the following
for each `GOOS=windows` and `GOOS=linux`:

- In the Xray source, run `go-licenses save ./main --save_path <work>/xray-<OS>-notices`.
- In the sing-box source, add the upstream binary's build tags to GOFLAGS:
  `-tags=with_gvisor,with_quic,with_dhcp,with_wireguard,with_utls,with_acme,with_clash_api,with_tailscale`,
  then run `go-licenses save ./cmd/sing-box --save_path <work>/sing-<OS>-notices`.

Review collector output. Version 2.0.1 reports unknown licenses for short GPL
declarations in sing-box, sing, sing-vmess, fswatch, sing-anytls and for the short
Apache declaration in go-nat-pmp. Their actual declarations are preserved with
full license texts and listed explicitly in the inventory. Other errors must be
resolved; do not treat arbitrary collector failure as successful review. Assembly
files produce inspection warnings; module notices and the separately verified
embedded Wintun binary notice are retained.

After reviewing these results, collect the saved files and independently checked
module notices with:

```sh
python scripts/collect-engine-notices.py --go /path/to/go --sources <work>
python scripts/license-notices.py
```

The script compares every dependency module sum against the exact binary's build
metadata, requires notice files for every module, and records the binary SHA-256
pins, tool versions, module versions/sums, build tags and manual declarations.
Review the resulting archive/index and public diff before distributing changed
dependencies. The upstream engine source archives and build instructions remain
linked in ENGINES.md; no Android patches or custom engine builds are shipped.
