//! Native subscription IO. Secret URLs/device headers never enter snapshots,
//! errors, events or logs. HTTPS certificate validation is never disabled.
use crate::{mutate, AppState, Snapshot};
use moth_core::{Quota, Subscription, SubscriptionInput};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager};
use uuid::Uuid;

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[tauri::command]
pub fn save_subscription(
    input: SubscriptionInput,
    state: tauri::State<AppState>,
) -> Result<Snapshot, String> {
    mutate(&state, |library| {
        library.put_subscription(input).map_err(str::to_owned)?;
        library.validate().map_err(str::to_owned)
    })
}
#[tauri::command]
pub fn delete_subscription(id: Uuid, state: tauri::State<AppState>) -> Result<Snapshot, String> {
    mutate(&state, |library| {
        library.remove_subscription(id).map_err(str::to_owned)
    })
}
#[tauri::command]
pub fn get_device_id(state: tauri::State<AppState>) -> Result<String, String> {
    // Persist before exposing the identifier. It is a random installation ID,
    // not a hardware fingerprint, and is excluded from portable backups.
    mutate(&state, |_| Ok(()))?;
    Ok(state
        .library
        .lock()
        .map_err(|_| "Хранилище недоступно")?
        .device_id
        .to_string())
}
#[tauri::command]
pub fn toggle_favorite(id: Uuid, state: tauri::State<AppState>) -> Result<Snapshot, String> {
    mutate(&state, |library| {
        if !library.profiles.iter().any(|p| p.id == id) {
            return Err("Сервер не найден".into());
        }
        if library.favorites.contains(&id) {
            library.favorites.retain(|p| *p != id);
        } else {
            library.favorites.push(id);
        }
        Ok(())
    })
}

async fn download(
    source: &Subscription,
    device: Uuid,
) -> Result<(Vec<moth_core::Profile>, Quota), String> {
    let original =
        moth_core::subscription_url(&source.url, source.allow_http).map_err(str::to_owned)?;
    let mut destination = original.clone();
    let mut builder = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(25));
    // A dedicated authenticated inbound belongs to the active service-owned
    // tunnel and has proxy as its only outbound. No system-proxy/direct fallback.
    let proxy = if source.vpn_only {
        #[cfg(any(windows, target_os = "linux"))]
        {
            match moth_ipc::ServiceClient::production()
                .call(moth_ipc::Command::SubscriptionProxy {})
                .await
            {
                Ok(reply) => match reply.result {
                    moth_ipc::Reply::SubscriptionProxy(proxy) => Some(proxy),
                    _ => {
                        return Err(
                            "Для обновления нужен ваш активный VPN и служба 0.3.4 или новее".into(),
                        )
                    }
                },
                Err(_) => {
                    return Err(
                        "VPN-only: защищённый канал службы недоступен; прямой запрос запрещён"
                            .into(),
                    )
                }
            }
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        return Err("VPN-only на этой платформе недоступен".into());
    } else {
        None
    };
    if let Some(proxy) = &proxy {
        builder = builder.proxy(
            reqwest::Proxy::all(format!("http://127.0.0.1:{}", proxy.port))
                .map_err(|_| "Не удалось создать защищённый канал")?
                .basic_auth(&proxy.username, &proxy.password),
        );
    }
    let client = builder
        .build()
        .map_err(|_| "Не удалось создать HTTPS-клиент")?;
    for hop in 0..6 {
        // No HTTPS->HTTP downgrade even when the original HTTP source was
        // explicitly allowed. Reject credentials and fragments on every hop.
        moth_core::subscription_url(destination.as_str(), source.allow_http)
            .map_err(str::to_owned)?;
        if original.scheme() == "https" && destination.scheme() != "https" {
            return Err("Перенаправление с HTTPS на HTTP запрещено".into());
        }
        let mut request = client.get(destination.clone());
        for (key, value) in source
            .headers_for_device(&destination, device, &moth_platform::subscription_device())
            .map_err(str::to_owned)?
        {
            request = request.header(key, value);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| "Подписка не загружена: сеть, сертификат или таймаут")?;
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("")
        };
        if ["x-hwid-max-devices-reached", "x-hwid-limit"]
            .iter()
            .any(|key| header(key).eq_ignore_ascii_case("true"))
        {
            return Err("Провайдер сообщил о лимите устройств. Обратитесь к провайдеру".into());
        }
        if header("x-hwid-not-supported").eq_ignore_ascii_case("true") {
            return Err("Провайдер требует HWID. Разрешите его передачу этой подписке".into());
        }
        match response.status().as_u16(){
            301|302|303|307|308=>{
                if hop==5{return Err("Слишком много перенаправлений".into());}
                let location=header("location");
                destination=destination.join(location).map_err(|_|"Некорректное перенаправление подписки")?;
                if location.is_empty(){return Err("Нет адреса перенаправления".into());}
                continue;
            },
            401|403=>return Err("Доступ к подписке отклонён. Проверьте ссылку и регистрацию устройства у провайдера".into()),
            200=>{},
            _=>return Err("Сервис подписки вернул ошибку. Последний рабочий список сохранён".into()),
        }
        let quota = Quota::parse(header("subscription-userinfo"));
        if response
            .content_length()
            .is_some_and(|n| n > 2 * 1024 * 1024)
        {
            return Err("Ответ подписки больше 2 МБ".into());
        }
        let mut body = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Ответ подписки не загружен полностью")?
        {
            if body.len() + chunk.len() > 2 * 1024 * 1024 {
                return Err("Ответ подписки больше 2 МБ".into());
            }
            body.extend_from_slice(&chunk);
        }
        let text = std::str::from_utf8(&body).map_err(|_| "Ответ подписки должен быть в UTF-8")?;
        // Atomic parser: an unsupported member rejects the entire refresh.
        let profiles = moth_core::import_links(text).map_err(|e| e.to_string())?;
        return Ok((profiles, quota));
    }
    Err("Слишком много перенаправлений".into())
}

#[tauri::command]
pub async fn refresh_subscription(id: Uuid, app: tauri::AppHandle) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    let _gate = state
        .subscription_gate
        .try_lock()
        .map_err(|_| "Другое обновление подписки ещё выполняется")?;
    let handle = app.clone();
    let (source, device) = tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        mutate(&state, |library| {
            let source = library
                .subscriptions
                .iter_mut()
                .find(|s| s.id == id)
                .ok_or("Подписка не найдена")?;
            source.generation = source
                .generation
                .checked_add(1)
                .ok_or("Счётчик обновлений исчерпан")?;
            source.last_attempt = Some(now());
            Ok(())
        })?;
        let library = state.library.lock().map_err(|_| "Хранилище недоступно")?;
        Ok::<_, String>((
            library
                .subscriptions
                .iter()
                .find(|s| s.id == id)
                .ok_or("Подписка не найдена")?
                .clone(),
            library.device_id,
        ))
    })
    .await
    .map_err(|_| "Обновление подписки прервано")??;
    let outcome = download(&source, device).await;
    let app = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let mut failure = outcome.as_ref().err().cloned();
        let snapshot = mutate(&state, |library| {
            if !library
                .subscriptions
                .iter()
                .any(|s| s.id == id && s.generation == source.generation)
            {
                return Err("Подписка изменилась во время обновления".into());
            }
            match outcome {
                Ok((profiles, quota)) => {
                    let mut candidate = library.clone();
                    match candidate.apply_subscription(
                        id,
                        source.generation,
                        profiles,
                        quota,
                        now(),
                    ) {
                        Ok(()) => {
                            *library = candidate;
                            Ok(())
                        }
                        Err(error) => {
                            failure = Some(error.into());
                            library
                                .subscriptions
                                .iter_mut()
                                .find(|s| s.id == id)
                                .unwrap()
                                .last_error = Some(error.into());
                            Ok(())
                        }
                    }
                }
                Err(error) => {
                    library
                        .subscriptions
                        .iter_mut()
                        .find(|s| s.id == id)
                        .unwrap()
                        .last_error = Some(error);
                    Ok(())
                }
            }
        })?;
        let _ = app.emit("moth-library-changed", ());
        if let Some(error) = failure {
            Err(error)
        } else {
            Ok(snapshot)
        }
    })
    .await
    .map_err(|_| "Обновление подписки прервано")?;
    result
}

pub fn schedule(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;
            let due = {
                let state = app.state::<AppState>();
                state
                    .library
                    .lock()
                    .map(|library| {
                        library
                            .subscriptions
                            .iter()
                            .filter(|s| {
                                s.refresh_hours > 0
                                    && now().saturating_sub(
                                        s.last_success.or(s.last_attempt).unwrap_or(0),
                                    ) >= u64::from(s.refresh_hours) * 3600
                                    && now().saturating_sub(s.last_attempt.unwrap_or(0)) >= 300
                            })
                            .map(|s| s.id)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            };
            for id in due {
                let _ = refresh_subscription(id, app.clone()).await;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    #[ignore = "requires the disposable hosted HTTPS provider fixture and its trusted CA"]
    async fn native_provider_fixture() {
        assert!(std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true"));
        let base = std::env::var("MOTH_PROVIDER_BASE").expect("fixture endpoint required");
        let device = Uuid::parse_str("00000000-0000-4000-8000-000000000009").unwrap();
        let make = |case: &str, consent: bool| Subscription {
            id: Uuid::new_v4(),
            name: "Fixture".into(),
            url: format!("{base}/{case}"),
            profiles: Vec::new(),
            send_hwid: consent,
            send_device_info: consent,
            user_agent: "Moth/fixture".into(),
            refresh_hours: 0,
            vpn_only: false,
            allow_http: false,
            generation: 0,
            last_attempt: None,
            last_success: None,
            last_error: None,
            quota: Quota::default(),
        };
        for (case, consent, count) in [
            ("opt-out", false, 2),
            ("device", true, 2),
            ("redirect", true, 2),
            ("cross", true, 2),
            ("json", false, 1),
            ("yaml", false, 1),
        ] {
            let result = download(&make(case, consent), device).await;
            assert!(result.is_ok(), "synthetic provider request failed");
            let (profiles, quota) = result.unwrap();
            assert!(profiles.len() == count);
            assert!(quota.used_bytes == Some(30) && quota.total_bytes == Some(100));
        }
        for case in ["partial", "limit", "denied", "oversize", "downgrade"] {
            let result = download(&make(case, false), device).await;
            assert!(result.is_err());
            let error = result.err().unwrap();
            assert!(
                !error.contains(&base)
                    && !error.contains(&device.to_string())
                    && !error.contains("synthetic")
            );
        }
        let mut source = make("opt-out", false);
        source.vpn_only = true;
        assert!(
            download(&source, device).await.is_err(),
            "VPN-only must not fall back when no owned tunnel exists"
        );
    }
}
