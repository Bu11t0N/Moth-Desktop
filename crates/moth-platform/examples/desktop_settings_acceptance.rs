//! Actual per-user settings, exclusively under a disposable CI test account.
fn main() {
    assert_eq!(std::env::var("GITHUB_ACTIONS").as_deref(), Ok("true"));
    #[cfg(target_os = "linux")]
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "An ordinary test account is required"
    );
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Security::{
            Authorization::ConvertStringSidToSidW, CheckTokenMembership,
        };
        let text: Vec<u16> = "S-1-5-32-544".encode_utf16().chain(Some(0)).collect();
        let mut sid = std::ptr::null_mut();
        let mut member = 1;
        assert_ne!(ConvertStringSidToSidW(text.as_ptr(), &mut sid), 0);
        assert_ne!(
            CheckTokenMembership(std::ptr::null_mut(), sid, &mut member),
            0
        );
        windows_sys::Win32::Foundation::LocalFree(sid);
        assert_eq!(member, 0, "An ordinary test account is required");
    }
    let exe = std::env::current_exe().unwrap();
    assert!(
        !moth_platform::autostart::status(&exe, None)
            .unwrap()
            .registered,
        "Fresh test account required"
    );
    let change = moth_platform::autostart::set(true, &exe, None).unwrap();
    let owner = change.ownership().unwrap().to_owned();
    assert!(
        moth_platform::autostart::status(&exe, Some(&owner))
            .unwrap()
            .current_target
    );
    let other = exe.with_file_name(if cfg!(windows) {
        "external-fixture.exe"
    } else {
        "external-fixture"
    });
    let external = moth_platform::autostart::set(true, &other, Some(&owner)).unwrap();
    assert!(moth_platform::autostart::set(false, &exe, Some(&owner)).is_err());
    assert!(
        moth_platform::autostart::status(&exe, Some(&owner))
            .unwrap()
            .conflict
    );
    external.undo().unwrap();
    change.undo().unwrap();
    assert!(
        !moth_platform::autostart::status(&exe, None)
            .unwrap()
            .registered
    );
    let journal = zeroize::Zeroizing::new(moth_platform::system_proxy::prepare(10808).unwrap());
    let result = std::panic::catch_unwind(|| {
        moth_platform::system_proxy::apply(&journal).unwrap();
        assert!(
            moth_platform::system_proxy::status(Some(&journal))
                .unwrap()
                .owned
        );
        // A second, externally changed port must survive restoration of the
        // first assignment; the second journal then restores the first.
        let changed = zeroize::Zeroizing::new(moth_platform::system_proxy::prepare(10809).unwrap());
        moth_platform::system_proxy::apply(&changed).unwrap();
        assert!(moth_platform::system_proxy::restore(&journal).is_err());
        assert!(
            moth_platform::system_proxy::status(Some(&changed))
                .unwrap()
                .owned
        );
        moth_platform::system_proxy::restore(&changed).unwrap();
    });
    moth_platform::system_proxy::restore(&journal).unwrap();
    assert!(
        !moth_platform::system_proxy::status(Some(&journal))
            .unwrap()
            .owned
    );
    result.unwrap();
    println!("Ordinary-user login registration, foreign-edit preservation and owned OS proxy restoration passed.");
}
