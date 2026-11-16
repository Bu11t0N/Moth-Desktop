//! Backups stay native until encrypted. Preview returns counts and an opaque,
//! expiring token; a concurrent library change invalidates confirmation/undo.
use crate::{mutate, AppState, Snapshot};
use moth_core::{BackupSummary, Library};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::Manager;
use uuid::Uuid;

#[derive(Default)]
pub struct BackupState(Mutex<Transactions>);
#[derive(Default)]
struct Transactions {
    preview: Option<Staged>,
    undo: Option<Staged>,
}
struct Staged {
    token: Uuid,
    base: [u8; 32],
    library: Library,
    created: Instant,
}
impl Staged {
    fn valid(&self, token: Uuid, library: &Library) -> Result<(), String> {
        if self.token != token || self.created.elapsed() > Duration::from_secs(600) {
            return Err("Предварительный просмотр истёк. Откройте копию заново".into());
        }
        if self.base != fingerprint(library)? {
            return Err("Библиотека изменилась. Повторите предварительный просмотр".into());
        }
        Ok(())
    }
}
fn fingerprint(library: &Library) -> Result<[u8; 32], String> {
    let data = serde_json::to_vec(library).map_err(|_| "Не удалось проверить библиотеку")?;
    Ok(Sha256::digest(data).into())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    token: Uuid,
    summary: BackupSummary,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Restored {
    snapshot: Snapshot,
    undo_token: Uuid,
}

#[tauri::command]
pub async fn export_backup(password: String, app: tauri::AppHandle) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let library = state
            .library
            .lock()
            .map_err(|_| "Хранилище недоступно")?
            .clone();
        moth_core::export_backup(&library, &password).map_err(str::to_owned)
    })
    .await
    .map_err(|_| "Не удалось создать резервную копию".to_owned())?
}

#[tauri::command]
pub async fn preview_backup(
    text: String,
    password: String,
    app: tauri::AppHandle,
) -> Result<Preview, String> {
    if text.len() > 8 * 1024 * 1024 || password.len() > 1024 {
        return Err("Резервная копия слишком большая".into());
    }
    // Capture before KDF work, so a scheduled refresh during decryption cannot
    // be overwritten by a previously prepared replacement.
    let base = fingerprint(
        &*app
            .state::<AppState>()
            .library
            .lock()
            .map_err(|_| "Хранилище недоступно")?,
    )?;
    tauri::async_runtime::spawn_blocking(move || {
        let decoded = moth_core::read_backup(&text, &password).map_err(str::to_owned)?;
        let token = Uuid::new_v4();
        let summary = decoded.summary();
        app.state::<BackupState>()
            .0
            .lock()
            .map_err(|_| "Хранилище недоступно")?
            .preview = Some(Staged {
            token,
            base,
            library: decoded.library,
            created: Instant::now(),
        });
        Ok(Preview { token, summary })
    })
    .await
    .map_err(|_| "Не удалось прочитать резервную копию".to_owned())?
}

#[tauri::command]
pub fn discard_backup(state: tauri::State<BackupState>) -> Result<(), String> {
    state.0.lock().map_err(|_| "Хранилище недоступно")?.preview = None;
    Ok(())
}

#[tauri::command]
pub fn confirm_backup(
    token: Uuid,
    replace: bool,
    state: tauri::State<AppState>,
    backups: tauri::State<BackupState>,
) -> Result<Restored, String> {
    let mut transactions = backups.0.lock().map_err(|_| "Хранилище недоступно")?;
    let staged = transactions
        .preview
        .take()
        .ok_or("Откройте резервную копию заново")?;
    let undo_token = Uuid::new_v4();
    let mut undo = None;
    let snapshot = mutate(&state, |library| {
        staged.valid(token, library)?;
        let next =
            moth_core::merge_backup(library, staged.library, replace).map_err(str::to_owned)?;
        undo = Some(Staged {
            token: undo_token,
            base: fingerprint(&next)?,
            library: library.clone(),
            created: Instant::now(),
        });
        *library = next;
        Ok(())
    })?;
    transactions.undo = undo;
    Ok(Restored {
        snapshot,
        undo_token,
    })
}

#[tauri::command]
pub fn undo_backup(
    token: Uuid,
    state: tauri::State<AppState>,
    backups: tauri::State<BackupState>,
) -> Result<Snapshot, String> {
    let mut transactions = backups.0.lock().map_err(|_| "Хранилище недоступно")?;
    let staged = transactions
        .undo
        .take()
        .ok_or("Нет доступной отмены восстановления")?;
    mutate(&state, |library| {
        staged.valid(token, library)?;
        *library = staged.library;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn confirmation_and_undo_refuse_stale_library_or_expired_token() {
        let mut library = Library::default();
        let token = Uuid::new_v4();
        let mut staged = Staged {
            token,
            base: fingerprint(&library).unwrap(),
            library: library.clone(),
            created: Instant::now(),
        };
        assert!(staged.valid(token, &library).is_ok());
        assert!(staged.valid(Uuid::new_v4(), &library).is_err());
        library.add(moth_core::import_links("trojan://synthetic@example.com:443").unwrap());
        assert!(staged.valid(token, &library).is_err());
        staged.base = fingerprint(&library).unwrap();
        staged.created = Instant::now() - Duration::from_secs(601);
        assert!(staged.valid(token, &library).is_err());
    }
}
