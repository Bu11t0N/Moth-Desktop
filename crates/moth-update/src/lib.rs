//! Signed release metadata and package verification shared by GUI and root
//! maintenance. No shell execution, OS installation, profiles or runtime trust
//! anchor/endpoint overrides are exposed here.
use base64::{engine::general_purpose::STANDARD, Engine as _};
use minisign_verify::{PublicKey, Signature};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path, time::Duration};
use tokio::io::AsyncWriteExt;

const PUBLIC_KEY: &str = include_str!("../../../packaging/updater.pub");
const RELEASE_FEED: &str = "https://api.github.com/repos/Bu11t0N/Moth-Desktop/releases?per_page=20";
const RELEASE_BASE: &str = "https://github.com/Bu11t0N/Moth-Desktop/releases/download";
const MAX_MANIFEST: usize = 64 * 1024;
const MAX_SIGNATURE: usize = 2048;
const MAX_PACKAGE: u64 = 256 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("Не удалось получить обновление по защищённому соединению")]
    Network,
    #[error("Подпись обновления не прошла проверку")]
    Signature,
    #[error("Некорректные или несовместимые сведения об обновлении")]
    Manifest,
    #[error("Версия обновления должна быть новее установленной")]
    NotNewer,
    #[error("Размер обновления не соответствует допустимому")]
    Size,
    #[error("Файл обновления повреждён или изменён")]
    Integrity,
    #[error("Не удалось прочитать или сохранить файл обновления")]
    Io,
    #[error("Обновление для этой платформы недоступно")]
    Unsupported,
}

#[derive(Clone, Copy)]
enum Platform {
    Windows,
    Linux,
}
impl Platform {
    fn current() -> Result<Self, UpdateError> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "x86_64") => Ok(Self::Windows),
            ("linux", "x86_64") => Ok(Self::Linux),
            _ => Err(UpdateError::Unsupported),
        }
    }
    fn filename(self, version: &str) -> String {
        match self {
            Self::Windows => format!("Moth-{version}-windows-x64-setup.exe"),
            Self::Linux => format!("Moth-{version}-linux-x64.deb"),
        }
    }
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    application: String,
    version: String,
    protocol: u16,
    notes: String,
    platforms: Platforms,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Platforms {
    #[serde(rename = "windows-x86_64")]
    windows: Artifact,
    #[serde(rename = "linux-x86_64")]
    linux: Artifact,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    file: String,
    url: String,
    size: u64,
    sha256: String,
    signature: String,
}
impl Manifest {
    fn artifact(&self, platform: Platform) -> &Artifact {
        match platform {
            Platform::Windows => &self.platforms.windows,
            Platform::Linux => &self.platforms.linux,
        }
    }
    fn validate(&self, current: &Version) -> Result<(), UpdateError> {
        let version = release_version(&self.version)?;
        if version <= *current {
            return Err(UpdateError::NotNewer);
        }
        if self.schema != 1
            || self.application != "org.mothvpn.desktop"
            || self.protocol != 2
            || self.notes.len() > 4096
        {
            return Err(UpdateError::Manifest);
        }
        for platform in [Platform::Windows, Platform::Linux] {
            let artifact = self.artifact(platform);
            if artifact.file != platform.filename(&self.version)
                || artifact.url != release_url(&self.version, &artifact.file)
                || artifact.sha256.len() != 64
                || !artifact
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(UpdateError::Manifest);
            }
            if artifact.size == 0 || artifact.size > MAX_PACKAGE {
                return Err(UpdateError::Size);
            }
            signature(&artifact.signature)?;
        }
        Ok(())
    }
}
fn release_version(text: &str) -> Result<Version, UpdateError> {
    if text.len() > 64 {
        return Err(UpdateError::Manifest);
    }
    let version = Version::parse(text).map_err(|_| UpdateError::Manifest)?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        return Err(UpdateError::Manifest);
    }
    Ok(version)
}
fn release_url(version: &str, file: &str) -> String {
    format!("{RELEASE_BASE}/v{version}/{file}")
}
fn public_key(encoded: &str) -> Result<PublicKey, UpdateError> {
    let bytes = STANDARD
        .decode(encoded.trim())
        .map_err(|_| UpdateError::Signature)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| UpdateError::Signature)?;
    let line = text.lines().nth(1).ok_or(UpdateError::Signature)?;
    PublicKey::from_base64(line).map_err(|_| UpdateError::Signature)
}
fn signature(encoded: &str) -> Result<Signature, UpdateError> {
    if encoded.len() > MAX_SIGNATURE {
        return Err(UpdateError::Signature);
    }
    let bytes = STANDARD
        .decode(encoded.trim())
        .map_err(|_| UpdateError::Signature)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| UpdateError::Signature)?;
    Signature::decode(text).map_err(|_| UpdateError::Signature)
}

/// Only signature verification constructs this type. Root maintenance repeats
/// construction from the untrusted request bytes using its own compiled key.
#[derive(Clone)]
pub struct VerifiedUpdate {
    manifest: Manifest,
    bytes: Vec<u8>,
    signed: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSummary {
    pub version: String,
    pub notes: String,
    pub file: String,
    pub size: u64,
}
impl VerifiedUpdate {
    pub fn from_signed(
        bytes: &[u8],
        signed: &str,
        current_version: &str,
    ) -> Result<Self, UpdateError> {
        Self::with_key(bytes, signed, current_version, &public_key(PUBLIC_KEY)?)
    }
    fn with_key(
        bytes: &[u8],
        signed: &str,
        current_version: &str,
        key: &PublicKey,
    ) -> Result<Self, UpdateError> {
        if bytes.is_empty() || bytes.len() > MAX_MANIFEST {
            return Err(UpdateError::Size);
        }
        key.verify(bytes, &signature(signed)?, false)
            .map_err(|_| UpdateError::Signature)?;
        let manifest: Manifest =
            serde_json::from_slice(bytes).map_err(|_| UpdateError::Manifest)?;
        manifest.validate(&release_version(current_version)?)?;
        Ok(Self {
            manifest,
            bytes: bytes.to_vec(),
            signed: signed.into(),
        })
    }
    pub fn summary(&self) -> Result<UpdateSummary, UpdateError> {
        let artifact = self.manifest.artifact(Platform::current()?);
        Ok(UpdateSummary {
            version: self.manifest.version.clone(),
            notes: self.manifest.notes.clone(),
            file: artifact.file.clone(),
            size: artifact.size,
        })
    }
    pub fn manifest_bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn manifest_signature(&self) -> &str {
        &self.signed
    }
    pub fn version(&self) -> &str {
        &self.manifest.version
    }
    pub fn verify_package(&self, path: &Path) -> Result<(), UpdateError> {
        self.verify_package_for(path, Platform::current()?, &public_key(PUBLIC_KEY)?)
    }
    fn verify_package_for(
        &self,
        path: &Path,
        platform: Platform,
        key: &PublicKey,
    ) -> Result<(), UpdateError> {
        let artifact = self.manifest.artifact(platform);
        let mut file = File::open(path).map_err(|_| UpdateError::Io)?;
        let metadata = file.metadata().map_err(|_| UpdateError::Io)?;
        if !metadata.is_file() || metadata.len() != artifact.size {
            return Err(UpdateError::Size);
        }
        let signed = signature(&artifact.signature)?;
        let mut verifier = key
            .verify_stream(&signed)
            .map_err(|_| UpdateError::Signature)?;
        let mut hash = Sha256::new();
        let mut total = 0u64;
        let mut buffer = [0; 65536];
        loop {
            let count = file.read(&mut buffer).map_err(|_| UpdateError::Io)?;
            if count == 0 {
                break;
            }
            total += count as u64;
            if total > artifact.size {
                return Err(UpdateError::Size);
            }
            verifier.update(&buffer[..count]);
            hash.update(&buffer[..count]);
        }
        if total != artifact.size || format!("{:x}", hash.finalize()) != artifact.sha256 {
            return Err(UpdateError::Integrity);
        }
        verifier.finalize().map_err(|_| UpdateError::Signature)?;
        Ok(())
    }
    pub async fn download(
        &self,
        destination: &Path,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<(), UpdateError> {
        let artifact = self.manifest.artifact(Platform::current()?);
        let mut response = client()?
            .get(&artifact.url)
            .send()
            .await
            .map_err(|_| UpdateError::Network)?;
        if !response.status().is_success() {
            return Err(UpdateError::Network);
        }
        if response
            .content_length()
            .is_some_and(|size| size != artifact.size)
        {
            return Err(UpdateError::Size);
        }
        // Never truncate an existing file, including a symbolic link.
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)
            .await
            .map_err(|_| UpdateError::Io)?;
        let mut total = 0u64;
        progress(0, artifact.size);
        while let Some(chunk) = response.chunk().await.map_err(|_| UpdateError::Network)? {
            total += chunk.len() as u64;
            if total > artifact.size {
                return Err(UpdateError::Size);
            }
            file.write_all(&chunk).await.map_err(|_| UpdateError::Io)?;
            progress(total, artifact.size);
        }
        file.sync_all().await.map_err(|_| UpdateError::Io)?;
        drop(file);
        if total != artifact.size {
            return Err(UpdateError::Size);
        }
        self.verify_package(destination)
    }
}

fn client() -> Result<reqwest::Client, UpdateError> {
    reqwest::Client::builder()
        .https_only(true)
        .no_proxy()
        .user_agent(concat!("Moth-Desktop/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(180))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let host = attempt.url().host_str().unwrap_or("");
            let expected = reqwest::Url::parse(RELEASE_BASE).ok();
            if attempt.previous().len() < 8
                && attempt.url().scheme() == "https"
                && (expected.as_ref().and_then(|url| url.host_str()) == Some(host)
                    || [
                        "api.github.com",
                        "release-assets.githubusercontent.com",
                        "objects.githubusercontent.com",
                    ]
                    .contains(&host))
            {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|_| UpdateError::Network)
}
async fn bounded_get(
    client: &reqwest::Client,
    url: &str,
    limit: usize,
) -> Result<Vec<u8>, UpdateError> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| UpdateError::Network)?;
    if !response.status().is_success() {
        return Err(UpdateError::Network);
    }
    if response
        .content_length()
        .is_some_and(|length| length > limit as u64)
    {
        return Err(UpdateError::Size);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| UpdateError::Network)? {
        if bytes.len() + chunk.len() > limit {
            return Err(UpdateError::Size);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    assets: Vec<ReleaseAsset>,
}
#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

/// GitHub is discovery only. The metadata and selected package each need a valid
/// signature, exact repository URL, compatible protocol and monotonic version.
pub async fn check(current_version: &str) -> Result<Option<VerifiedUpdate>, UpdateError> {
    let current = release_version(current_version)?;
    let client = client()?;
    let bytes = bounded_get(&client, RELEASE_FEED, 2 * 1024 * 1024).await?;
    let releases: Vec<Release> =
        serde_json::from_slice(&bytes).map_err(|_| UpdateError::Manifest)?;
    let mut candidates: Vec<_> = releases
        .into_iter()
        .filter_map(|release| {
            let version = release_version(release.tag_name.strip_prefix('v')?).ok()?;
            (!release.draft && version > current).then_some((version, release))
        })
        .collect();
    candidates.sort_by(|left, right| right.0.cmp(&left.0));
    for (version, release) in candidates {
        let version = version.to_string();
        let metadata = release
            .assets
            .iter()
            .filter(|asset| asset.name == "moth-update.json")
            .collect::<Vec<_>>();
        let signed = release
            .assets
            .iter()
            .filter(|asset| asset.name == "moth-update.json.sig")
            .collect::<Vec<_>>();
        // Older manual-only releases have no signed updater metadata.
        if metadata.is_empty() && signed.is_empty() {
            continue;
        }
        if metadata.len() != 1
            || signed.len() != 1
            || metadata[0].browser_download_url != release_url(&version, "moth-update.json")
            || signed[0].browser_download_url != release_url(&version, "moth-update.json.sig")
        {
            return Err(UpdateError::Manifest);
        }
        let bytes = bounded_get(&client, &metadata[0].browser_download_url, MAX_MANIFEST).await?;
        let signed = bounded_get(&client, &signed[0].browser_download_url, MAX_SIGNATURE).await?;
        let update = VerifiedUpdate::from_signed(
            &bytes,
            std::str::from_utf8(&signed).map_err(|_| UpdateError::Signature)?,
            current_version,
        )?;
        if update.version() != version {
            return Err(UpdateError::Manifest);
        }
        return Ok(Some(update));
    }
    Ok(None)
}

#[cfg(test)]
mod tests;
