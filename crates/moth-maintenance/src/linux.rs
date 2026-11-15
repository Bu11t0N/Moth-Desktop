use super::*;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
pub(super) struct ChildGroup(u32);
pub(super) fn finish(group: &ChildGroup) -> Result<()> {
    unsafe {
        libc::kill(-(group.0 as i32), libc::SIGKILL);
    }
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let mut status = 0;
        while unsafe { libc::waitpid(-(group.0 as i32), &mut status, libc::WNOHANG) } > 0 {}
        if unsafe { libc::kill(-(group.0 as i32), 0) } != 0 {
            return if std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
                Ok(())
            } else {
                Err(())
            };
        }
        if Instant::now() >= deadline {
            return Err(());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
impl Drop for ChildGroup {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-(self.0 as i32), libc::SIGKILL);
        }
    }
}
pub(super) fn spawn(mut command: Command) -> Result<(std::process::Child, ChildGroup)> {
    // Reap orphaned apt/dpkg descendants instead of handing them to PID 1 while
    // deciding whether it is safe to start a rollback transaction.
    if unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) } != 0 {
        return Err(());
    }
    let child = command.process_group(0).spawn().map_err(|_| ())?;
    let group = ChildGroup(child.id());
    Ok((child, group))
}
pub(super) const PACKAGE: &str = "update.deb";
pub(super) fn service() -> Result<PathBuf> {
    Ok("/usr/lib/moth-desktop/moth-service".into())
}
pub(super) fn environment(command: &mut Command) -> Result<()> {
    command
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .env("DEBIAN_FRONTEND", "noninteractive");
    Ok(())
}
pub(super) fn validate_self() -> Result<()> {
    if unsafe { libc::geteuid() } != 0 {
        return Err(());
    }
    let executable = std::env::current_exe().map_err(|_| ())?;
    if executable != Path::new("/usr/lib/moth-desktop/moth-maintenance") {
        return Err(());
    }
    for path in [
        Path::new("/"),
        Path::new("/usr"),
        Path::new("/usr/lib"),
        Path::new("/usr/lib/moth-desktop"),
        executable.as_path(),
    ] {
        secure::validate_protected_path(path).map_err(|_| ())?;
    }
    Ok(())
}
pub(super) fn install() -> Result<()> {
    validate_self()
}
fn wait_gui(job: &Path) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if command(
            job,
            Path::new("/usr/bin/pgrep"),
            &["-x".as_ref(), "moth-desktop".as_ref()],
            Duration::from_secs(3),
        )
        .is_err()
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
fn package_version(job: &Path, package: &Path, expected: &str) -> Result<()> {
    let output = command(
        job,
        Path::new("/usr/bin/dpkg-deb"),
        &[
            "--field".as_ref(),
            package.as_os_str(),
            "Package".as_ref(),
            "Version".as_ref(),
            "Architecture".as_ref(),
        ],
        Duration::from_secs(10),
    )?;
    let text = std::str::from_utf8(&output).map_err(|_| ())?;
    // dpkg-deb prints named fields when more than one field was requested.
    let expected = format!("Package: moth-desktop\nVersion: {expected}\nArchitecture: amd64\n");
    if text != expected {
        return Err(());
    }
    Ok(())
}
pub(super) fn validate_package(job: &Path, package: &Path, version: &str) -> Result<()> {
    package_version(job, package, version)
}
pub(super) fn apply(
    job: &Path,
    package: &Path,
    previous: &str,
    next: &str,
    running: bool,
) -> Result<()> {
    wait_gui(job)?;
    // dpkg-repack reconstructs the installed package through the package
    // manager; no direct overwrite of dpkg-owned executable files is used.
    command(
        job,
        Path::new("/usr/bin/dpkg-repack"),
        &["moth-desktop".as_ref()],
        Duration::from_secs(120),
    )?;
    let mut backups = fs::read_dir(job)
        .map_err(|_| ())?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path != package && path.extension().is_some_and(|extension| extension == "deb")
        });
    let backup = backups.next().ok_or(())?;
    if backups.next().is_some() {
        return Err(());
    }
    secure::validate_protected_path(&backup).map_err(|_| ())?;
    package_version(job, &backup, previous)?;
    fs::set_permissions(&backup, fs::Permissions::from_mode(0o600)).map_err(|_| ())?;
    report(job, "installing", next)?;
    let install = command(
        job,
        Path::new("/usr/bin/apt-get"),
        &[
            "--yes".as_ref(),
            "--no-remove".as_ref(),
            "--no-install-recommends".as_ref(),
            "install".as_ref(),
            package.as_os_str(),
        ],
        Duration::from_secs(300),
    );
    if !children_clean() {
        report(job, "recoveryFailed", previous)?;
        return Err(());
    }
    if install.is_ok() && healthy(job, next).is_ok() {
        return report(job, "complete", next);
    }
    report(job, "rollingBack", previous)?;
    let rollback = command(
        job,
        Path::new("/usr/bin/apt-get"),
        &[
            "--yes".as_ref(),
            "--no-remove".as_ref(),
            "--allow-downgrades".as_ref(),
            "install".as_ref(),
            backup.as_os_str(),
        ],
        Duration::from_secs(300),
    );
    if rollback.is_ok() && !running {
        command(
            job,
            Path::new("/usr/bin/systemctl"),
            &["stop".as_ref(), "moth-desktop.service".as_ref()],
            Duration::from_secs(30),
        )?;
    }
    if children_clean() && rollback.is_ok() && restored_previous(job, previous, running).is_ok() {
        report(job, "failed", previous)?;
    } else {
        report(job, "recoveryFailed", previous)?;
    }
    Err(())
}
