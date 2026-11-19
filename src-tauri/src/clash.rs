//! Preview and refresh user-controlled policy data outside the service account.
use crate::{mutate, subscriptions, AppState, Snapshot};
use moth_core::{ClashPolicy, PolicyInspection, PolicyPreview, SavedClashPolicy, Subscription};
use moth_ipc::{Command, ConnectionState, Reply, TunnelPhase};
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};
use uuid::Uuid;
use zeroize::Zeroizing;

pub struct Staged {
    token: Uuid,
    document: Zeroizing<String>,
    source: Option<Subscription>,
    quota: moth_core::Quota,
    created: Instant,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub token: Uuid,
    pub summary: PolicyPreview,
}
fn inspect(text: &str) -> Result<PolicyInspection, String> {
    moth_core::inspect_clash_policy(text)
        .map_err(str::to_owned)?
        .ok_or_else(|| "This input is not a Clash policy document".into())
}

#[tauri::command]
pub async fn preview_clash(
    text: Option<String>,
    subscription: Option<Uuid>,
    app: tauri::AppHandle,
) -> Result<Option<Preview>, String> {
    let state = app.state::<AppState>();
    let (document, source, quota) = match (text, subscription) {
        (Some(text), None) => (Zeroizing::new(text), None, Default::default()),
        (None, Some(id)) => {
            let (source, device) = {
                let l = state.library.lock().map_err(|_| "Storage unavailable")?;
                (
                    l.subscriptions
                        .iter()
                        .find(|s| s.id == id)
                        .cloned()
                        .ok_or("Subscription missing")?,
                    l.device_id,
                )
            };
            let route = crate::proxy::native_route(&app, source.vpn_only).await?;
            let (document, quota) =
                subscriptions::download_document(&source, device, route).await?;
            (document, Some(source), quota)
        }
        _ => return Err("Choose either a local document or a subscription".into()),
    };
    let Some(inspection) = moth_core::inspect_clash_policy(&document).map_err(str::to_owned)?
    else {
        return Ok(None);
    };
    let token = Uuid::new_v4();
    *state
        .policy_preview
        .lock()
        .map_err(|_| "Preview unavailable")? = Some(Staged {
        token,
        document,
        source,
        quota,
        created: Instant::now(),
    });
    Ok(Some(Preview {
        token,
        summary: inspection.preview,
    }))
}

async fn prepare(
    app: &tauri::AppHandle,
    mut policy: ClashPolicy,
    vpn_only: bool,
) -> Result<(String, String, Option<u64>), String> {
    let state = app.state::<AppState>();
    let original = policy.stored_yaml().map_err(str::to_owned)?;
    let requests = policy.providers();
    let refresh = requests.iter().map(|r| r.interval).min();
    if !requests.is_empty() {
        let route = crate::proxy::native_route(app, vpn_only).await?;
        let mut client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(12));
        if let Some(proxy) = crate::proxy::reqwest_proxy(route.as_ref())? {
            client = client.proxy(proxy);
        }
        let client = client.build().map_err(|_| "Provider client unavailable")?;
        for request in requests {
            let mut response = client
                .get(&request.url)
                .send()
                .await
                .map_err(|_| format!("Provider {} unavailable", request.name))?;
            if !response.status().is_success() {
                return Err(format!("Provider {} returned an error", request.name));
            }
            let mut bytes = Zeroizing::new(Vec::new());
            while let Some(chunk) = response
                .chunk()
                .await
                .map_err(|_| "Incomplete provider response")?
            {
                if bytes.len() + chunk.len() > 192 * 1024 {
                    return Err("Provider response exceeds 192 KiB".into());
                }
                bytes.extend_from_slice(&chunk);
            }
            let body =
                std::str::from_utf8(&bytes).map_err(|_| "Provider response must be UTF-8")?;
            policy
                .with_provider(&request, body)
                .map_err(|error| format!("Provider {}: {error}", request.name))?;
        }
    }
    state
        .engines
        .validate_clash(&policy)
        .await
        .map_err(|e| e.to_string())?;
    Ok((
        original,
        policy.stored_yaml().map_err(str::to_owned)?,
        refresh,
    ))
}

fn current(library: &moth_core::Library, source: Option<&Subscription>) -> Result<(), String> {
    if source.is_some_and(|old| {
        !library
            .subscriptions
            .iter()
            .any(|s| s.id == old.id && s.generation == old.generation)
    }) {
        return Err("Subscription changed during preview or refresh".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn apply_clash(
    token: Uuid,
    name: String,
    nodes_only: bool,
    app: tauri::AppHandle,
) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    let _gate = state
        .subscription_gate
        .try_lock()
        .map_err(|_| "A refresh is already in progress")?;
    let staged = {
        let mut slot = state
            .policy_preview
            .lock()
            .map_err(|_| "Preview unavailable")?;
        if !slot
            .as_ref()
            .is_some_and(|p| p.token == token && p.created.elapsed() < Duration::from_secs(600))
        {
            return Err("Run the preview again".into());
        }
        slot.take().unwrap()
    };
    let parsed = inspect(&staged.document)?;
    let source = staged.source;
    if nodes_only {
        if parsed.nodes.is_empty() {
            return Err("No standalone nodes in this policy".into());
        }
        return mutate(&state, |l| {
            current(l, source.as_ref())?;
            // Nodes-only is an explicit one-time import, independent of any policy cache.
            l.add(parsed.nodes);
            l.validate().map_err(str::to_owned)
        });
    }
    let policy = parsed
        .policy
        .ok_or("The full profile has incompatible fields")?;
    let name = source.as_ref().map_or(name, |s| s.name.clone());
    let subscription = source.as_ref().map(|s| s.id);
    let (original, resolved, refresh_seconds) =
        prepare(&app, policy, source.as_ref().is_some_and(|s| s.vpn_only)).await?;
    mutate(&state, |l| {
        current(l, source.as_ref())?;
        let previous = subscription.and_then(|id| {
            l.clash_policies
                .iter()
                .position(|p| p.subscription == Some(id))
        });
        let now = subscriptions::now();
        let record = SavedClashPolicy {
            id: previous.map_or_else(Uuid::new_v4, |i| l.clash_policies[i].id),
            name,
            subscription,
            original,
            resolved,
            refreshed_at: now,
            last_attempt: now,
            refresh_seconds,
            refresh_failed: false,
        };
        if let Some(index) = previous {
            l.clash_policies[index] = record;
        } else {
            l.clash_policies.push(record);
        }
        if let Some(s) = l
            .subscriptions
            .iter_mut()
            .find(|s| Some(s.id) == subscription)
        {
            s.last_attempt = Some(now);
            s.last_success = Some(now);
            s.last_error = None;
            s.quota = staged.quota;
        }
        l.validate().map_err(str::to_owned)
    })
}

pub async fn refresh(
    id: Uuid,
    fetch_subscription: bool,
    app: tauri::AppHandle,
) -> Result<Snapshot, String> {
    let state = app.state::<AppState>();
    let _gate = state
        .subscription_gate
        .try_lock()
        .map_err(|_| "A refresh is already in progress")?;
    let (saved, source, device) = {
        let l = state.library.lock().map_err(|_| "Storage unavailable")?;
        let p = l
            .clash_policies
            .iter()
            .find(|p| p.id == id)
            .cloned()
            .ok_or("Clash profile missing")?;
        let source = l
            .subscriptions
            .iter()
            .find(|s| Some(s.id) == p.subscription)
            .cloned();
        (p, source, l.device_id)
    };
    let outcome = async {
        let mut quota = None;
        let document = if let Some(source) = source.as_ref().filter(|_| fetch_subscription) {
            let route = crate::proxy::native_route(&app, source.vpn_only).await?;
            let (text, result_quota) =
                subscriptions::download_document(source, device, route).await?;
            quota = Some(result_quota);
            text
        } else {
            Zeroizing::new(saved.original.clone())
        };
        let parsed = inspect(&document)?
            .policy
            .ok_or("Updated profile contains incompatible fields")?;
        let (original, resolved, refresh) =
            prepare(&app, parsed, source.as_ref().is_some_and(|s| s.vpn_only)).await?;
        Ok::<_, String>((original, resolved, refresh, quota))
    }
    .await;
    let error = outcome.as_ref().err().cloned();
    let result = mutate(&state, |l| {
        current(l, source.as_ref())?;
        let target = l
            .clash_policies
            .iter_mut()
            .find(|p| {
                p.id == id && p.original == saved.original && p.subscription == saved.subscription
            })
            .ok_or("Clash profile changed during refresh")?;
        let now = subscriptions::now();
        target.last_attempt = now;
        target.refresh_failed = outcome.is_err();
        let mut updated_quota = None;
        if let Ok((original, resolved, interval, quota)) = outcome {
            target.original = original;
            target.resolved = resolved;
            target.refresh_seconds = interval;
            target.refreshed_at = now;
            updated_quota = quota;
        }
        if fetch_subscription {
            if let Some(s) = l
                .subscriptions
                .iter_mut()
                .find(|s| Some(s.id) == saved.subscription)
            {
                s.last_attempt = Some(now);
                s.last_error = error.clone();
                if let Some(quota) = updated_quota {
                    s.quota = quota;
                    s.last_success = Some(now);
                }
            }
        }
        l.validate().map_err(str::to_owned)
    })?;
    let _ = app.emit("moth-library-changed", ());
    if let Some(error) = error {
        Err(error)
    } else {
        Ok(result)
    }
}

pub async fn refresh_due(app: &tauri::AppHandle) {
    let now = subscriptions::now();
    let due = app
        .state::<AppState>()
        .library
        .lock()
        .map(|l| {
            l.clash_policies
                .iter()
                .filter(|p| {
                    p.refresh_seconds
                        .is_some_and(|n| now.saturating_sub(p.last_attempt) >= n)
                })
                .map(|p| p.id)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    for id in due {
        let _ = refresh(id, false, app.clone()).await;
    }
}

#[tauri::command]
pub async fn refresh_clash(id: Uuid, app: tauri::AppHandle) -> Result<Snapshot, String> {
    refresh(id, true, app).await
}

#[tauri::command]
pub fn remove_clash(id: Uuid, state: tauri::State<AppState>) -> Result<Snapshot, String> {
    mutate(&state, |l| {
        l.clash_policies.retain(|p| p.id != id);
        Ok(())
    })
}

#[tauri::command]
pub async fn connect_clash(id: Uuid, app: tauri::AppHandle) -> Result<ConnectionState, String> {
    let lifecycle = app.state::<crate::tray::DesktopLifecycle>();
    let _lease = lifecycle.controls.read().await;
    if lifecycle.busy.load(std::sync::atomic::Ordering::Acquire) {
        return Err("Wait for the previous connection to stop".into());
    }
    let state = app.state::<AppState>();
    let (document, settings, prefs) = {
        let l = state.library.lock().map_err(|_| "Storage unavailable")?;
        let p = l
            .clash_policies
            .iter()
            .find(|p| p.id == id)
            .ok_or("Clash profile missing")?;
        if l.system_proxy_ownership.is_some() {
            return Err("Restore the previous system proxy first".into());
        }
        (
            p.resolved.clone(),
            l.settings.clone(),
            l.proxy_preferences.clone(),
        )
    };
    let guard = moth_ipc::ServiceClient::production()
        .call(Command::GuardStatus {})
        .await
        .map_err(|e| e.to_string())?;
    if !matches!(guard.result, Reply::Guard(s) if !s.enabled) {
        return Err("Clash policies require a confirmed disabled kill switch".into());
    }
    let policy = inspect(&document)?
        .policy
        .ok_or("The saved policy is incompatible")?;
    policy
        .validate_connection(&settings, prefs.mode)
        .map_err(str::to_owned)?;
    prefs.validate().map_err(str::to_owned)?;
    if prefs.mode == moth_core::ConnectionMode::Proxy || prefs.system_proxy {
        mutate(&state, |l| {
            l.proxy_session_requested = true;
            Ok(())
        })?;
    }
    let result = crate::tunnel_call(Command::BeginClashPolicy {
        document,
        settings: Box::new(settings),
        mode: prefs.mode,
        listener: prefs.public_listener().cloned(),
    })
    .await?;
    if result.phase != TunnelPhase::Connected || !result.owned_by_caller {
        return Err("The imported policy could not verify its connection. Check its routes and the health-check destination".into());
    }
    if prefs.system_proxy {
        if let Err(error) = crate::proxy::apply_system(&app, prefs.listener.port).await {
            let _ = crate::tunnel_call(Command::Disconnect {}).await;
            let _ = crate::proxy::restore_owned(&app).await;
            return Err(error);
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn clash_groups(
    group: Option<String>,
    member: Option<String>,
) -> Result<Vec<moth_core::GroupState>, String> {
    let command = match (group, member) {
        (None, None) => Command::PolicyGroups {},
        (Some(group), Some(member)) => Command::ChoosePolicyMember { group, member },
        _ => return Err("Choose a group and an exit".into()),
    };
    let selecting = matches!(command, Command::ChoosePolicyMember { .. });
    let reply = moth_ipc::ServiceClient::production()
        .call(command)
        .await
        .map_err(|e| e.to_string())?;
    match reply.result {
        Reply::PolicyGroups { groups } => Ok(groups),
        Reply::Failure {
            code: moth_ipc::ErrorCode::Unsupported | moth_ipc::ErrorCode::NotOwner,
        } if !selecting => Ok(Vec::new()),
        _ => Err("Policy groups are unavailable".into()),
    }
}
