#[cfg(any(windows, target_os = "linux"))]
fn main() {
    assert!(std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true"));
    use moth_ipc::maintenance::{TunnelLease, UpdateLease};
    match std::env::args().nth(1).as_deref() {
        Some("private-create") => {
            use std::io::Write;
            let path = moth_ipc::maintenance::state_directory()
                .unwrap()
                .join(format!("acceptance-private-{}", std::process::id()));
            let mut file = moth_ipc::maintenance::create_private_file(&path).unwrap();
            file.write_all(b"synthetic private staging input").unwrap();
            drop(file);
            println!("{}", path.display());
            return;
        }
        Some("private-denied") => {
            let path = std::env::args_os().nth(2).expect("private acceptance path");
            assert_eq!(
                std::fs::read(path).unwrap_err().kind(),
                std::io::ErrorKind::PermissionDenied
            );
            println!("Ordinary user cannot read private maintenance input");
            return;
        }
        Some("blocked") => assert!(UpdateLease::acquire().is_err()),
        Some("available") => {
            let exclusive = UpdateLease::acquire().expect("exclusive update lease");
            assert!(TunnelLease::acquire().is_err());
            drop(exclusive);
            let shared = TunnelLease::acquire().expect("shared tunnel lease");
            assert!(UpdateLease::acquire().is_err());
            drop(shared);
        }
        _ => panic!("Expected blocked or available"),
    }
    println!("Real kernel maintenance lease verified");
}
#[cfg(not(any(windows, target_os = "linux")))]
fn main() {
    panic!("Unsupported platform")
}
