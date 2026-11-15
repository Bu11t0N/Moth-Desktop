use super::*;

const FIXTURE_KEY: &str = include_str!("fixtures/fixture.pub");
const MANIFEST: &[u8] = include_bytes!("fixtures/manifest.json");
const SIGNED: &str = include_str!("fixtures/manifest.json.sig");
const PACKAGE: &[u8] = include_bytes!("fixtures/package.bin");

#[test]
fn official_signer_vectors_verify_and_changes_unsigned_data_and_wrong_keys_fail() {
    let key = public_key(FIXTURE_KEY).unwrap();
    let update = VerifiedUpdate::with_key(MANIFEST, SIGNED, "0.3.2", &key).unwrap();
    assert_eq!(update.version(), "0.3.3");
    assert!(matches!(
        VerifiedUpdate::from_signed(MANIFEST, SIGNED, "0.3.2"),
        Err(UpdateError::Signature)
    ));
    let mut modified = MANIFEST.to_vec();
    modified.push(b' ');
    assert!(matches!(
        VerifiedUpdate::with_key(&modified, SIGNED, "0.3.2", &key),
        Err(UpdateError::Signature)
    ));
    assert!(VerifiedUpdate::with_key(MANIFEST, "", "0.3.2", &key).is_err());
    assert!(matches!(
        VerifiedUpdate::with_key(MANIFEST, SIGNED, "0.3.3", &key),
        Err(UpdateError::NotNewer)
    ));
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("package.bin");
    std::fs::write(&path, PACKAGE).unwrap();
    for platform in [Platform::Windows, Platform::Linux] {
        update.verify_package_for(&path, platform, &key).unwrap();
        assert!(update
            .verify_package_for(&path, platform, &public_key(PUBLIC_KEY).unwrap())
            .is_err());
    }
    let mut modified = PACKAGE.to_vec();
    modified[0] ^= 1;
    std::fs::write(&path, &modified).unwrap();
    assert!(matches!(
        update.verify_package_for(&path, Platform::Windows, &key),
        Err(UpdateError::Integrity)
    ));
    modified.push(0);
    std::fs::write(&path, modified).unwrap();
    assert!(matches!(
        update.verify_package_for(&path, Platform::Windows, &key),
        Err(UpdateError::Size)
    ));
}

#[test]
fn manifest_policy_rejects_incompatible_protocol_wrong_application_and_redirected_packages() {
    let current = release_version("0.3.2").unwrap();
    for (field, value) in [
        ("protocol", serde_json::json!(3)),
        ("application", serde_json::json!("another.application")),
    ] {
        let mut json: serde_json::Value = serde_json::from_slice(MANIFEST).unwrap();
        json[field] = value;
        let manifest: Manifest = serde_json::from_value(json).unwrap();
        assert!(matches!(
            manifest.validate(&current),
            Err(UpdateError::Manifest)
        ));
    }
    for url in ["http://github.com/Bu11t0N/Moth-Desktop/releases/download/v0.3.3/Moth-0.3.3-windows-x64-setup.exe","https://example.com/package.exe","file:///tmp/package.exe"] {
        let mut json:serde_json::Value=serde_json::from_slice(MANIFEST).unwrap();
        json["platforms"]["windows-x86_64"]["url"]=serde_json::json!(url);
        assert!(matches!(serde_json::from_value::<Manifest>(json).unwrap().validate(&current),Err(UpdateError::Manifest)));
    }
    assert!(VerifiedUpdate::from_signed(&vec![0; MAX_MANIFEST + 1], SIGNED, "0.3.2").is_err());
    assert!(signature(&"x".repeat(MAX_SIGNATURE + 1)).is_err());
}
