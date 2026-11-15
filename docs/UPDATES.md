# Signed update candidate

The expanded 0.3.3 candidate includes a signed-update implementation. It is not
a released update channel: actual two-build installation/rollback acceptance and
production release publication remain gated in [STATUS.md](STATUS.md).

The Settings page checks the fixed Moth Desktop GitHub release feed. The shared
Rust verifier accepts only bounded metadata signed by the compiled public key,
this application identifier, IPC 2 and a strictly newer three-part version. Both
platform artifacts have exact versioned repository URLs, sizes, SHA-256 and
independent package signatures. HTTPS and SHA-256 alone are insufficient. There
is no runtime key/endpoint override. The signing format is the official Tauri
signer's minisign format; streaming verification checks complete package bytes.

Download progress reports actual bytes. Installation disconnects the caller's
VPN through the existing confirmed-stop barrier and requests Windows UAC or
Linux polkit authorization. Another account's connection is not disconnected:
the privileged verifier must acquire an exclusive protected kernel lease before
preparing any installation. Each tunnel holds a shared lease throughout startup
and its lifetime. The helper independently verifies the installed version,
metadata signature, copied package signature/hash and embedded package version.
Only the protected copy can execute; requests never select privileged commands
or output paths. The privileged helper does not read profile storage.

The ordinary-user GUI exits itself once the helper confirms verified preparation.
A temporary copy of the same GUI displays public installation/rollback status
without loading profiles or initializing the VPN adapter. It restarts the fixed
installed GUI as the original ordinary user after success; failures offer opening
the retained previous version. An unconfirmed recovery does not automatically
restart it. Remaining GUI processes cause installation refusal, not forced kills.

Windows applies the complete NSIS GUI/service/core transaction with its existing
health check and full-tree rollback. The running helper is stored separately in
the administrator-owned ProgramData Moth Desktop maintenance/version directory,
so replacing the Program Files application tree cannot lock its executable.
Same-version helper retries compare bytes and do not overwrite a running helper.
Installer child processes are suspended until assigned to a kill-on-close Job
Object. Linux uses apt/dpkg for every executable replacement and dpkg-repack for
a protected backup of the installed package. The old package is reinstalled on
failure, and its daemon health or prior stopped state is independently checked.
No direct replacement of dpkg-owned files is performed. Linux child process
groups are reaped before considering rollback. Only the Moth package is selected
for update; apt may satisfy its declared dependencies from configured OS sources.

Protected jobs retain public package copies, previous-package backups and bounded
credential-free status under the platform state directory. Profiles remain in
the same Windows account-bound DPAPI storage or Linux Secret Service collection.
Removal preserves profiles and maintenance backups rather than recursively
deleting user or unknown files. The updater key signs updates; it is not Windows
Authenticode. Private signing keys/passwords are never repository or release assets.
