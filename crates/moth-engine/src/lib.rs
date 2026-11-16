//! Verified external engines; no Tauri, privileged operations or OS routing changes.
mod job;
#[cfg(windows)]
mod tunnel;
#[cfg(windows)]
pub use tunnel::Tunnel;
#[cfg(target_os = "linux")]
mod linux_tunnel;
#[cfg(target_os = "linux")]
pub use linux_tunnel::{recover_network, Tunnel};
#[cfg(test)]
mod tests;
use job::ProcessJob;
use moth_core::{LocalBridge, Profile};

/// HTTPS health measurement forced through the authenticated active-tunnel
/// listener. This client has no direct or environment-proxy fallback.
pub async fn health_through_proxy(
    proxy: &moth_ipc::DownloadProxy,
    url: &str,
) -> Result<(), EngineError> {
    let proxy = reqwest::Proxy::all(format!("http://127.0.0.1:{}", proxy.port))
        .map_err(|_| EngineError::Probe)?
        .basic_auth(&proxy.username, &proxy.password);
    let client = reqwest::Client::builder()
        .no_proxy()
        .proxy(proxy)
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|_| EngineError::Probe)?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|_| EngineError::Probe)?;
    if !response.status().is_success() {
        return Err(EngineError::Probe);
    }
    Ok(())
}

fn protect_verifier(
    config: &mut serde_json::Value,
    policy: &moth_core::ConnectionSettings,
) -> Result<(), EngineError> {
    let path = std::env::current_exe()
        .map_err(|_| EngineError::Process)?
        .to_string_lossy()
        .into_owned();
    for app in &policy.split_apps {
        if (cfg!(windows) && !(app.len() > 3 && app.as_bytes()[1] == b':'))
            || (!cfg!(windows) && !app.starts_with('/'))
            || app.eq_ignore_ascii_case(&path)
        {
            return Err(EngineError::InvalidConfig);
        }
    }
    // The service's TUN-sourced startup verifier must not be excluded by an
    // only-selected-apps rule. This remains real TUN verification, not a probe.
    config["route"]["rules"]
        .as_array_mut()
        .ok_or(EngineError::InvalidConfig)?
        .insert(
            1,
            serde_json::json!({"process_path":[path],"action":"route","outbound":"proxy"}),
        );
    Ok(())
}
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::{
    io::AsyncWriteExt,
    process::{Child, Command},
    time::timeout,
};
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("Сетевые ядра не установлены")]
    Missing,
    #[error("Проверка целостности сетевого ядра не пройдена")]
    Integrity,
    #[error("Не удалось запустить сетевое ядро")]
    Process,
    #[error("Ядро отклонило параметры сервера")]
    InvalidConfig,
    #[error("Проверка не завершилась за отведённое время")]
    Timeout,
    #[error("Сервер не прошёл HTTPS-проверку")]
    Probe,
    #[error("Эта платформа пока не поддерживается")]
    Unsupported,
    #[error("Не удалось определить исходный сетевой адаптер; отключите другой VPN")]
    Network,
    #[error("Установка службы не готова или выполняется обновление")]
    Maintenance,
}

#[derive(Clone, Copy)]
pub enum Engine {
    Xray,
    SingBox,
}
impl Engine {
    fn name(self) -> &'static str {
        match self {
            Self::Xray => "xray",
            Self::SingBox => "sing-box",
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EngineLock {
    schema_version: u32,
    engines: BTreeMap<String, EnginePin>,
}
#[derive(Deserialize)]
struct EnginePin {
    version: String,
    platforms: BTreeMap<String, AssetPin>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssetPin {
    file: String,
    binary_sha256: String,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineStatus {
    pub name: String,
    pub version: String,
    pub available: bool,
    pub message: Option<String>,
}

#[derive(Clone)]
pub struct EngineSet {
    directory: PathBuf,
}
impl EngineSet {
    pub fn new(directory: PathBuf) -> Self {
        Self { directory }
    }
    pub fn platform() -> Result<&'static str, EngineError> {
        match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "x86_64") => Ok("win32-x64"),
            ("linux", "x86_64") => Ok("linux-x64"),
            _ => Err(EngineError::Unsupported),
        }
    }
    fn lock() -> Result<EngineLock, EngineError> {
        let lock: EngineLock = serde_json::from_str(include_str!("../../../engines.lock.json"))
            .map_err(|_| EngineError::Integrity)?;
        if lock.schema_version != 1 {
            return Err(EngineError::Integrity);
        }
        Ok(lock)
    }
    /// Every launch verifies the executable against a pin baked into this binary.
    /// Runtime engine directories are never accepted from the webview.
    pub fn verified_path(&self, engine: Engine) -> Result<PathBuf, EngineError> {
        let lock = Self::lock()?;
        let asset = lock
            .engines
            .get(engine.name())
            .and_then(|p| p.platforms.get(Self::platform().ok()?))
            .ok_or(EngineError::Unsupported)?;
        let path = self.directory.join(&asset.file);
        let mut file = std::fs::File::open(&path).map_err(|_| EngineError::Missing)?;
        if file.metadata().map_err(|_| EngineError::Integrity)?.len() > 128 * 1024 * 1024 {
            return Err(EngineError::Integrity);
        }
        let mut hash = Sha256::new();
        let mut buf = [0u8; 65536];
        loop {
            let n = file.read(&mut buf).map_err(|_| EngineError::Integrity)?;
            if n == 0 {
                break;
            }
            hash.update(&buf[..n]);
        }
        if format!("{:x}", hash.finalize()) != asset.binary_sha256 {
            return Err(EngineError::Integrity);
        }
        std::fs::canonicalize(path).map_err(|_| EngineError::Missing)
    }
    pub fn status(&self) -> Vec<EngineStatus> {
        [Engine::Xray, Engine::SingBox]
            .into_iter()
            .map(|e| {
                let result = self.verified_path(e);
                let version = Self::lock()
                    .ok()
                    .and_then(|mut l| l.engines.remove(e.name()))
                    .map(|p| p.version)
                    .unwrap_or_default();
                EngineStatus {
                    name: e.name().into(),
                    version,
                    available: result.is_ok(),
                    message: result.err().map(|e| e.to_string()),
                }
            })
            .collect()
    }
    async fn spawn_config(
        &self,
        engine: Engine,
        config: &serde_json::Value,
        check: bool,
    ) -> Result<EngineProcess, EngineError> {
        let executable = self.verified_path(engine)?;
        let mut cmd = Command::new(&executable);
        cmd.current_dir(executable.parent().ok_or(EngineError::Missing)?)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        // Engine overrides accept multiple spellings, including dotted names.
        // Inherit only OS essentials; never merge unrelated config directories.
        cmd.env_clear();
        for (key, value) in std::env::vars_os() {
            if [
                "SYSTEMROOT",
                "WINDIR",
                "TEMP",
                "TMP",
                "PATH",
                "LANG",
                "LC_ALL",
            ]
            .contains(&key.to_string_lossy().to_ascii_uppercase().as_str())
            {
                cmd.env(key, value);
            }
        }
        match engine {
            Engine::Xray => {
                cmd.args(["run", "-format", "json", "-config", "stdin:"]);
                if check {
                    cmd.arg("-test");
                }
            }
            Engine::SingBox => {
                cmd.args([if check { "check" } else { "run" }, "-c", "stdin"]);
            }
        }
        #[cfg(windows)]
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW; no console flashes.
        #[cfg(target_os = "linux")]
        {
            // SAFETY: only async-signal-safe syscalls between fork and exec.
            // Check the original parent too, closing the death-before-prctl race.
            let parent = unsafe { libc::getpid() };
            unsafe {
                cmd.pre_exec(move || {
                    if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    if libc::getppid() != parent {
                        return Err(std::io::Error::other("Engine parent exited"));
                    }
                    Ok(())
                });
            }
        }
        let mut child = cmd.spawn().map_err(|_| EngineError::Process)?;
        let job = match ProcessJob::attach(&child) {
            Ok(j) => j,
            Err(e) => {
                let _ = child.kill().await;
                return Err(e);
            }
        };
        let mut stdin = child.stdin.take().ok_or(EngineError::Process)?;
        let bytes =
            Zeroizing::new(serde_json::to_vec(config).map_err(|_| EngineError::InvalidConfig)?);
        let mut process = EngineProcess { child, _job: job };
        if timeout(Duration::from_secs(5), stdin.write_all(&bytes))
            .await
            .map_err(|_| EngineError::Timeout)?
            .is_err()
        {
            let _ = process.stop().await;
            return Err(EngineError::Process);
        }
        drop(stdin); // Xray must see EOF to finish loading stdin config.
        Ok(process)
    }
    pub async fn validate(
        &self,
        engine: Engine,
        config: &serde_json::Value,
    ) -> Result<(), EngineError> {
        let mut process = self.spawn_config(engine, config, true).await?;
        let status = timeout(Duration::from_secs(10), process.child.wait())
            .await
            .map_err(|_| EngineError::Timeout)?
            .map_err(|_| EngineError::Process)?;
        if !status.success() {
            return Err(EngineError::InvalidConfig);
        }
        Ok(())
    }
    pub async fn validate_profile(&self, profile: &Profile) -> Result<(), EngineError> {
        let bridge = LocalBridge {
            port: 15491,
            credential: Uuid::new_v4(),
        };
        self.validate(
            Engine::Xray,
            &moth_core::xray_bridge_config(profile, &bridge),
        )
        .await?;
        self.validate(Engine::SingBox, &moth_core::sing_box_tun_config(&bridge))
            .await
    }
    /// Measures real HTTPS through a short-lived, authenticated loopback proxy.
    /// Does not change system proxy, DNS, routes or firewall. Success is not VPN protection.
    pub async fn probe(&self, profile: &Profile) -> Result<ProbeResult, EngineError> {
        self.probe_url(profile, "https://cp.cloudflare.com/generate_204")
            .await
    }
    async fn probe_url(&self, profile: &Profile, url: &str) -> Result<ProbeResult, EngineError> {
        self.validate(Engine::Xray, &moth_core::xray_check_config(profile))
            .await?;
        let listener =
            std::net::TcpListener::bind("127.0.0.1:0").map_err(|_| EngineError::Process)?;
        let port = listener
            .local_addr()
            .map_err(|_| EngineError::Process)?
            .port();
        drop(listener);
        let password = Uuid::new_v4().simple().to_string();
        let mut config = moth_core::xray_check_config(profile);
        config["inbounds"] = serde_json::json!([{"listen":"127.0.0.1","port":port,"protocol":"socks","settings":{"auth":"password","accounts":[{"user":"moth","pass":password}],"udp":false}}]);
        let mut process = self.spawn_config(Engine::Xray, &config, false).await?;
        let start = Instant::now();
        let result = async {
            loop {
                if process
                    .child
                    .try_wait()
                    .map_err(|_| EngineError::Process)?
                    .is_some()
                {
                    return Err(EngineError::Process);
                }
                if tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
                    .await
                    .is_ok()
                {
                    break;
                }
                if start.elapsed() > Duration::from_secs(3) {
                    return Err(EngineError::Timeout);
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
            let proxy = reqwest::Proxy::all(format!("socks5h://moth:{password}@127.0.0.1:{port}"))
                .map_err(|_| EngineError::Probe)?;
            let client = reqwest::Client::builder()
                .no_proxy()
                .proxy(proxy)
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(12))
                .build()
                .map_err(|_| EngineError::Probe)?;
            let request_started = Instant::now();
            let response = client
                .get(url)
                .send()
                .await
                .map_err(|_| EngineError::Probe)?;
            if response.status() != reqwest::StatusCode::NO_CONTENT {
                return Err(EngineError::Probe);
            }
            if process
                .child
                .try_wait()
                .map_err(|_| EngineError::Process)?
                .is_some()
            {
                return Err(EngineError::Process);
            }
            Ok(ProbeResult {
                profile_id: profile.id,
                latency_ms: request_started.elapsed().as_millis() as u64,
            })
        }
        .await;
        process.stop().await?;
        result
    }
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub profile_id: Uuid,
    pub latency_ms: u64,
}

struct EngineProcess {
    child: Child,
    _job: ProcessJob,
}
impl EngineProcess {
    async fn stop(&mut self) -> Result<(), EngineError> {
        if self
            .child
            .try_wait()
            .map_err(|_| EngineError::Process)?
            .is_none()
        {
            self.child.kill().await.map_err(|_| EngineError::Process)?;
        }
        Ok(())
    }
}

/// Resolve the fixed engine location used by the build/test harness.
pub fn development_directory() -> Result<PathBuf, EngineError> {
    Ok(Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/engines")
        .join(EngineSet::platform()?))
}
