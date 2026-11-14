//! A real storage boundary fixture for disposable Linux packaging acceptance.
#[cfg(target_os = "linux")]
fn main() {
    assert!(std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true"));
    assert!(std::env::var("MOTH_SECRET_SERVICE_ACCEPTANCE").as_deref() == Ok("1"));
    let store = moth_platform::ProfileStore::new(std::path::PathBuf::new());
    match std::env::args().nth(1).as_deref() {
        Some("write") => {
            assert!(store.load().unwrap().profiles.is_empty());
            let mut library = moth_core::Library::default();
            library.add(
                moth_core::import_links(
                    "trojan://synthetic-package-acceptance@example.com:443#Preserved",
                )
                .unwrap(),
            );
            store.save(&library).unwrap();
        }
        Some("verify") => {
            let library = store.load().unwrap();
            assert!(library.profiles.len() == 1);
            assert!(library.profiles[0].credential == "synthetic-package-acceptance");
            assert!(library.profiles[0].name == "Preserved");
            assert!(library.selected == Some(library.profiles[0].id));
        }
        _ => panic!("Expected write or verify"),
    }
    println!("Real Secret Service profile fixture verified");
}
#[cfg(not(target_os = "linux"))]
fn main() {
    panic!("Linux-only acceptance fixture");
}
