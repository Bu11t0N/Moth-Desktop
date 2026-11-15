//! Offline verification of the release trust anchor and this OS's package.
use base64::{engine::general_purpose::STANDARD, Engine as _};
use minisign_verify::{PublicKey, Signature};
use std::{fs, io::Read, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let directory = PathBuf::from(args.next().ok_or("release directory required")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let update = moth_update::VerifiedUpdate::from_signed(
        &fs::read(directory.join("moth-update.json"))?,
        &fs::read_to_string(directory.join("moth-update.json.sig"))?,
        "0.0.0",
    )?;
    if update.version() != env!("CARGO_PKG_VERSION") {
        return Err("signed metadata and source versions differ".into());
    }
    update.verify_package(&directory.join(update.summary()?.file))?;
    let encoded = STANDARD.decode(include_str!("../../../packaging/updater.pub").trim())?;
    let key = PublicKey::from_base64(
        std::str::from_utf8(&encoded)?
            .lines()
            .nth(1)
            .ok_or("invalid public key")?,
    )?;
    for suffix in ["windows-x64.zip", "windows-x64-setup.exe", "linux-x64.deb"] {
        let name = format!("Moth-{}-{suffix}", update.version());
        let encoded =
            STANDARD.decode(fs::read_to_string(directory.join(name.clone() + ".sig"))?.trim())?;
        let signature = Signature::decode(std::str::from_utf8(&encoded)?)?;
        let expected = format!("version:{}", update.version());
        let fields: Vec<_> = signature
            .trusted_comment()
            .split('\t')
            .filter(|field| field.starts_with("version:"))
            .collect();
        if fields != [expected.as_str()] {
            return Err("package signature version mismatch".into());
        }
        let mut verifier = key.verify_stream(&signature)?;
        let mut file = fs::File::open(directory.join(name))?;
        let mut buffer = [0; 65536];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            verifier.update(&buffer[..count]);
        }
        verifier.finalize()?;
    }
    println!("Production public key, signed version and native package verified offline.");
    Ok(())
}
