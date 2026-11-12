# Moth Desktop development

- Read docs/SPEC.ru.md, docs/ARCHITECTURE.md and docs/STATUS.md before changing functionality.
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
