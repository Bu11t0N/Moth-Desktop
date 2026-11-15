#[cfg(any(windows, target_os = "linux"))]
fn main() {
    assert!(std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true"));
    use moth_ipc::maintenance::{TunnelLease, UpdateLease};
    match std::env::args().nth(1).as_deref() {
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
