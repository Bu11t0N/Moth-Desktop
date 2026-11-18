These public test-only TLS credentials authenticate loopback protocol fixtures.
They are not a production key, trust anchor or OS certificate installation.
Only Rust test code includes them. The client configuration trusts this exact
self-signed certificate and still checks `protocol.fixture.moth.invalid`.
