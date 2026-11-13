# Moth Desktop development

- Read docs/SPEC.ru.md, docs/ARCHITECTURE.md, docs/STATUS.md and docs/ROADMAP.ru.md before changing functionality.
- Windows is the first supported runtime; retain a compiling Linux boundary.
- Keep Tauri out of moth-core and moth-platform. UI must consume summaries, not credentials/config JSON.
- Never claim connected/protected from UI state alone. No simulated VPN, fake latency or insecure persistence fallback.
- No network/OS DNS/routes/firewall changes during tests on the host. Use an isolated VM for tunnel acceptance.
- Import unsupported configurations explicitly fails; do not silently strip security/transport parameters.
- Do not log credentials, subscription URLs, raw imported strings or generated configurations.
- Preserve Android format attribution and GPL notices when porting code or fixtures.
- Rust formatting, relevant tests, Clippy, TypeScript build and UI checks must pass for implementation changes.
- Keep docs/STATUS.md honest: implemented, verified and planned are distinct.
- Do not automatically publish a signed/production release. Development source pushes and CI are expected.
- Keep release versions consistent across manifests, binaries and package names.
- Public documentation contains product behavior, architecture, reproducible validation and release information. Keep personal delivery instructions, workstation details, chat handoffs and local tool setup outside the repository.
- Review every public diff for personal names, machine names, local paths and private notes before pushing.
- Product text uses clear actions, settings and states. Avoid slogans, motivational copy, metaphors and unsupported claims.
