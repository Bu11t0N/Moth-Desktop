//! The desktop Secret Service is the only Linux persistence backend. The
//! application writes no serialized profiles to disk and has no plaintext fallback.
use super::{Envelope, Library, StoreError, Zeroizing};
use secret_service::{blocking::SecretService, EncryptionType};
use std::collections::HashMap;

fn attributes() -> HashMap<&'static str, &'static str> {
    HashMap::from([
        ("application", "org.mothvpn.desktop"),
        ("kind", "profile-library-v1"),
    ])
}

pub(super) fn load() -> Result<Library, StoreError> {
    // Blocking API: called from the GUI's synchronous command/setup boundary,
    // never from an async task. DH encrypts secrets on the session bus as well.
    let service = SecretService::connect(EncryptionType::Dh).map_err(|_| StoreError::Protection)?;
    let collection = service
        .get_default_collection()
        .map_err(|_| StoreError::Protection)?;
    if collection.is_locked().map_err(|_| StoreError::Protection)? {
        return Err(StoreError::Protection);
    }
    let items = collection
        .search_items(attributes())
        .map_err(|_| StoreError::Protection)?;
    if items.is_empty() {
        return Ok(Library::default());
    }
    if items.len() != 1 || items[0].is_locked().map_err(|_| StoreError::Protection)? {
        return Err(StoreError::Invalid);
    }
    let plain = Zeroizing::new(items[0].get_secret().map_err(|_| StoreError::Protection)?);
    if plain.len() > 8 * 1024 * 1024 {
        return Err(StoreError::Invalid);
    }
    let envelope: Envelope = serde_json::from_slice(&plain).map_err(|_| StoreError::Invalid)?;
    if envelope.version != 1 || envelope.library.validate().is_err() {
        return Err(StoreError::Invalid);
    }
    Ok(envelope.library)
}

pub(super) fn save(library: &Library) -> Result<(), StoreError> {
    library.validate().map_err(|_| StoreError::Invalid)?;
    let plain = Zeroizing::new(
        serde_json::to_vec(&Envelope {
            version: 1,
            library: library.clone(),
        })
        .map_err(|_| StoreError::Invalid)?,
    );
    if plain.len() > 8 * 1024 * 1024 {
        return Err(StoreError::Invalid);
    }
    let service = SecretService::connect(EncryptionType::Dh).map_err(|_| StoreError::Protection)?;
    let collection = service
        .get_default_collection()
        .map_err(|_| StoreError::Protection)?;
    if collection.is_locked().map_err(|_| StoreError::Protection)? {
        return Err(StoreError::Protection);
    }
    // Refuse ambiguous storage rather than overwriting several matching items.
    if collection
        .search_items(attributes())
        .map_err(|_| StoreError::Protection)?
        .len()
        > 1
    {
        return Err(StoreError::Invalid);
    }
    collection
        .create_item(
            "Moth Desktop profiles",
            attributes(),
            &plain,
            true,
            "application/json",
        )
        .map_err(|_| StoreError::Protection)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a disposable session bus with a real unlocked Secret Service"]
    fn real_secret_service_roundtrip_replace_and_corruption() {
        assert_eq!(
            std::env::var("MOTH_SECRET_SERVICE_ACCEPTANCE").as_deref(),
            Ok("1")
        );
        let service = SecretService::connect(EncryptionType::Dh).unwrap();
        let collection = service.get_default_collection().unwrap();
        assert!(collection.search_items(attributes()).unwrap().is_empty());
        let mut library = Library::default();
        library.add(
            moth_core::import_links("trojan://synthetic-acceptance-only@example.com:443#Test")
                .unwrap(),
        );
        save(&library).unwrap();
        assert_eq!(load().unwrap().profiles.len(), 1);
        library.remove(library.selected.unwrap());
        save(&library).unwrap();
        assert!(load().unwrap().profiles.is_empty());
        let item = collection
            .search_items(attributes())
            .unwrap()
            .pop()
            .unwrap();
        item.set_secret(b"corrupt", "application/json").unwrap();
        assert!(load().is_err());
        item.delete().unwrap();
        collection.lock().unwrap();
        assert!(load().is_err());
        assert!(save(&library).is_err());
    }
}
