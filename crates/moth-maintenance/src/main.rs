//! Fixed signed-package maintenance. No IPC command interpreter, user-selected
//! executable, shell, profile access, runtime public key or endpoint override.
#![cfg_attr(windows, windows_subsystem = "windows")]
use moth_ipc::maintenance as secure;
use moth_update::VerifiedUpdate;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use uuid::Uuid;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "linux")]
use linux as platform;
#[cfg(windows)]
use windows as platform;

type Result<T> = std::result::Result<T, ()>;
const VERSION: &str = env!("CARGO_PKG_VERSION");
const PACKAGE_LIMIT: u64 = 256 * 1024 * 1024;
static CHILD_CLEANUP_FAILED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);
fn children_clean() -> bool {
    !CHILD_CLEANUP_FAILED.load(std::sync::atomic::Ordering::Acquire)
}

fn bounded_file(path: &Path, limit: u64) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x00200000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let file = options.open(path).map_err(|_| ())?;
    let metadata = file.metadata().map_err(|_| ())?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(());
        }
    }
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > limit {
        return Err(());
    }
    Ok(file)
}
fn bytes(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    bounded_file(path, limit)?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() as u64 > limit {
        return Err(());
    }
    Ok(bytes)
}
fn copy(source: &Path, destination: &Path, limit: u64, private: bool) -> Result<()> {
    let source = bounded_file(source, limit)?;
    let mut destination = if private {
        secure::create_private_file(destination)
    } else {
        secure::create_protected_file(destination)
    }
    .map_err(|_| ())?;
    let size = std::io::copy(&mut source.take(limit + 1), &mut destination).map_err(|_| ())?;
    if size > limit {
        return Err(());
    }
    destination.sync_all().map_err(|_| ())
}
fn directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => secure::validate_protected_path(path).map_err(|_| ()),
        Ok(_) => Err(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            secure::create_protected_directory(path).map_err(|_| ())
        }
        Err(_) => Err(()),
    }
}
fn report(job: &Path, phase: &str, version: &str) -> Result<()> {
    // Durable phase evidence distinguishes a verified rollback from refusal
    // before installation. These files contain no profile/request data.
    let mut event =
        secure::create_protected_file(&job.join(format!("{phase}.event"))).map_err(|_| ())?;
    event.write_all(version.as_bytes()).map_err(|_| ())?;
    event.sync_all().map_err(|_| ())?;
    let next = job.join(format!("{}.next", Uuid::new_v4()));
    let mut file = secure::create_protected_file(&next).map_err(|_| ())?;
    serde_json::to_writer(
        &mut file,
        &serde_json::json!({"phase":phase,"version":version}),
    )
    .map_err(|_| ())?;
    file.write_all(b"\n").map_err(|_| ())?;
    file.sync_all().map_err(|_| ())?;
    drop(file);
    fs::rename(next, job.join("status.json")).map_err(|_| ())
}
/// Capture only bounded public command output in a protected temporary file.
/// A daemon inheriting a pipe cannot prevent the child deadline from firing.
fn command(
    job: &Path,
    program: &Path,
    arguments: &[&std::ffi::OsStr],
    timeout: Duration,
) -> Result<Vec<u8>> {
    if !children_clean() {
        return Err(());
    }
    let output = job.join(format!("{}.command.log", Uuid::new_v4()));
    let file = secure::create_protected_file(&output).map_err(|_| ())?;
    let errors = file.try_clone().map_err(|_| ())?;
    let mut command = Command::new(program);
    command
        .args(arguments)
        .env_clear()
        .env("LANG", "C")
        .current_dir(job)
        .stdin(Stdio::null())
        .stdout(Stdio::from(file))
        .stderr(Stdio::from(errors));
    platform::environment(&mut command)?;
    let (mut child, containment) = platform::spawn(command)?;
    let deadline = Instant::now() + timeout;
    let result = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(100)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    // Reap remaining descendants before reading output or considering rollback.
    // Killing just apt/NSIS could leave dpkg/PowerShell mutating files afterwards.
    if platform::finish(&containment).is_err() {
        CHILD_CLEANUP_FAILED.store(true, std::sync::atomic::Ordering::Release);
        return Err(());
    }
    drop(containment);
    let metadata = fs::metadata(&output).map_err(|_| ())?;
    let value = if metadata.len() == 0 {
        Ok(Vec::new())
    } else {
        bytes(&output, 65536)
    };
    if result && value.is_ok() {
        let _ = fs::remove_file(output);
    }
    if !result {
        return Err(());
    }
    value
}
fn healthy(job: &Path, expected: &str) -> Result<()> {
    let bytes = command(
        job,
        &platform::service()?,
        &["--status".as_ref()],
        Duration::from_secs(12),
    )?;
    let health: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| ())?;
    if health["version"] != expected
        || health["protocol"] != 2
        || health["systemTunnel"] != true
        || !health["engines"].as_array().is_some_and(|engines| {
            engines.len() == 2 && engines.iter().all(|engine| engine["available"] == true)
        })
    {
        return Err(());
    }
    Ok(())
}
fn restored_previous(job: &Path, expected: &str, running: bool) -> Result<()> {
    if running {
        return healthy(job, expected);
    }
    if !moth_ipc::service_is_stopped().map_err(|_| ())? {
        return Err(());
    }
    let version = command(
        job,
        &platform::service()?,
        &["--version".as_ref()],
        Duration::from_secs(10),
    )?;
    if std::str::from_utf8(&version).map_err(|_| ())?.trim() != expected {
        return Err(());
    }
    command(
        job,
        &platform::service()?,
        &["--check-installed".as_ref()],
        Duration::from_secs(10),
    )?;
    Ok(())
}
fn installed_version(job: &Path) -> Result<(String, bool)> {
    let service = platform::service()?;
    command(
        job,
        &service,
        &["--check-installed".as_ref()],
        Duration::from_secs(10),
    )?;
    let version = command(
        job,
        &service,
        &["--version".as_ref()],
        Duration::from_secs(10),
    )?;
    let version = std::str::from_utf8(&version)
        .map_err(|_| ())?
        .trim()
        .to_owned();
    // The root helper belongs to this installed coordinated build.
    if version != VERSION {
        return Err(());
    }
    if moth_ipc::service_is_stopped().map_err(|_| ())? {
        return Ok((version, false));
    }
    let state = command(
        job,
        &service,
        &["--connection-status".as_ref()],
        Duration::from_secs(10),
    )?;
    let state: serde_json::Value = serde_json::from_slice(&state).map_err(|_| ())?;
    if state["kind"] != "connection"
        || !matches!(state["phase"].as_str(), Some("disconnected" | "failed"))
    {
        return Err(());
    }
    Ok((version, true))
}
fn apply(request: &Path) -> Result<()> {
    platform::validate_self()?;
    // The input directory is untrusted. Its name is an opaque correlation ID,
    // never a command or destination path; all privileged destinations are fixed.
    let id = request
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(())?;
    if Uuid::parse_str(id).map_err(|_| ())?.to_string() != id {
        return Err(());
    }
    let state = secure::state_directory().map_err(|_| ())?;
    secure::validate_protected_path(&state).map_err(|_| ())?;
    let jobs = state.join("updates");
    directory(&jobs)?;
    let job = jobs.join(id);
    secure::create_protected_directory(&job).map_err(|_| ())?;
    report(&job, "verifying", VERSION)?;
    let result = (|| {
        let _lease = secure::UpdateLease::acquire().map_err(|_| ())?;
        let (previous, running) = installed_version(&job)?;
        let manifest = bytes(&request.join("manifest.json"), 64 * 1024)?;
        let signature = bytes(&request.join("manifest.sig"), 2048)?;
        let update = VerifiedUpdate::from_signed(
            &manifest,
            std::str::from_utf8(&signature).map_err(|_| ())?,
            &previous,
        )
        .map_err(|_| ())?;
        let package = job.join(platform::PACKAGE);
        // Do not expose input bytes through public maintenance status storage
        // before the independent package signature/hash check has succeeded.
        copy(&request.join("package.bin"), &package, PACKAGE_LIMIT, true)?;
        update.verify_package(&package).map_err(|_| ())?;
        platform::validate_package(&job, &package, update.version())?;
        report(&job, "preparing", update.version())?;
        // Never terminate a GUI from another account. The caller exits itself;
        // the package hooks additionally refuse any remaining GUI process.
        platform::apply(&job, &package, &previous, update.version(), running)
    })();
    if result.is_err() {
        // If installation ran, platform::apply already records whether rollback
        // was verified. Preserve that stronger outcome rather than overwrite it.
        let existing = bytes(&job.join("status.json"), 4096)?;
        let status: serde_json::Value = serde_json::from_slice(&existing).map_err(|_| ())?;
        if !matches!(status["phase"].as_str(), Some("failed" | "recoveryFailed")) {
            let phase = if matches!(status["phase"].as_str(), Some("installing" | "rollingBack")) {
                "recoveryFailed"
            } else {
                "failed"
            };
            report(&job, phase, VERSION)?;
        }
    }
    result
}
fn run() -> Result<()> {
    let arguments: Vec<_> = std::env::args_os().skip(1).collect();
    match arguments.first().and_then(|argument| argument.to_str()) {
        Some("--version") if arguments.len() == 1 => {
            println!("{VERSION}");
            Ok(())
        }
        Some("--install") if arguments.len() == 1 => platform::install(),
        Some("--apply") if arguments.len() == 2 => apply(&PathBuf::from(&arguments[1])),
        _ => Err(()),
    }
}
fn main() {
    if run().is_err() {
        std::process::exit(1)
    }
}
