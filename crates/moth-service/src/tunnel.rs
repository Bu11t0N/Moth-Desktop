use moth_ipc::{ErrorCode, Reply, TunnelPhase, TunnelState};
use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};
use tokio::sync::{watch, Mutex};

#[derive(Default)]
struct Connection {
    phase: TunnelPhase,
    owner: Option<String>,
    error: Option<ErrorCode>,
    session: Option<moth_engine::Session>,
    plan: moth_engine::SessionPlan,
    cancel: Option<watch::Sender<bool>>,
    profile: Option<moth_core::Profile>,
    candidates: Vec<moth_core::Profile>,
    policy: moth_core::ConnectionSettings,
    failures: u32,
    next_attempt: Option<Instant>,
    next_health: Option<Instant>,
}
#[derive(Default)]
pub struct Controller {
    guard: Mutex<Option<moth_engine::guard::Policy>>,
    inner: Mutex<Connection>,
    operation: Mutex<()>,
    generation: AtomicU64,
}
struct ConnectAttempt {
    generation: u64,
    automatic: bool,
    plan: moth_engine::SessionPlan,
}
impl Controller {
    pub async fn connection_status(&self, caller: &str) -> Reply {
        let state = self.inner.lock().await;
        Reply::Connection(moth_ipc::ConnectionState {
            phase: state.phase,
            owned_by_caller: state.owner.as_deref() == Some(caller),
            error: state.error,
            mode: state.plan.mode,
        })
    }
    pub async fn proxy_access(&self, caller: &str) -> Reply {
        let mut state = self.inner.lock().await;
        if state.owner.as_deref() != Some(caller) {
            return Reply::Failure {
                code: ErrorCode::NotOwner,
            };
        }
        if state.phase != TunnelPhase::Connected
            || !state.session.as_mut().is_some_and(|s| s.alive())
        {
            return Reply::Failure {
                code: ErrorCode::TunnelFailed,
            };
        }
        match &state.plan.public {
            Some(public) => Reply::ProxyAccess(public.access.clone()),
            None => Reply::Failure {
                code: ErrorCode::Unsupported,
            },
        }
    }
    pub async fn native_route(&self, caller: &str) -> Reply {
        let guard = self.guard.lock().await;
        let mut state = self.inner.lock().await;
        let route =
            if state.phase == TunnelPhase::Connected && state.owner.as_deref() == Some(caller) {
                state
                    .session
                    .as_mut()
                    .map_or(moth_ipc::NativeRoute::Blocked, |s| {
                        if s.alive() {
                            moth_ipc::NativeRoute::PrivateProxy(s.subscription_proxy())
                        } else {
                            moth_ipc::NativeRoute::Blocked
                        }
                    })
            } else if guard.is_none() && state.phase == TunnelPhase::Disconnected {
                moth_ipc::NativeRoute::Direct
            } else {
                moth_ipc::NativeRoute::Blocked
            };
        Reply::NativeRoute(route)
    }
    pub async fn traffic(&self, caller: &str) -> Reply {
        let state = self.inner.lock().await;
        if state.owner.as_deref() != Some(caller) || state.phase != TunnelPhase::Connected {
            return Reply::Failure {
                code: ErrorCode::NotOwner,
            };
        }
        let Some(api) = state.session.as_ref().and_then(|s| s.telemetry()) else {
            return Reply::Failure {
                code: ErrorCode::Unsupported,
            };
        };
        drop(state);
        let result = api.snapshot().await;
        let mut state = self.inner.lock().await;
        let same = state.owner.as_deref() == Some(caller)
            && state.phase == TunnelPhase::Connected
            && state.session.as_mut().is_some_and(|s| {
                s.alive()
                    && s.telemetry()
                        .is_some_and(|current| current.session() == api.session())
            });
        if !same {
            return Reply::Failure {
                code: ErrorCode::TunnelFailed,
            };
        }
        match result {
            Ok(snapshot) => Reply::Traffic(snapshot),
            Err(_) => Reply::Failure {
                code: ErrorCode::Unsupported,
            },
        }
    }
    pub async fn restore_guard(&self) -> Result<(), moth_ipc::IpcError> {
        let policy = tokio::task::spawn_blocking(|| {
            moth_engine::guard::restore()?;
            moth_engine::guard::load()
        })
        .await
        .map_err(|_| moth_ipc::IpcError::Unavailable)?
        .map_err(|_| moth_ipc::IpcError::Unavailable)?;
        *self.guard.lock().await = policy;
        Ok(())
    }
    pub async fn guard_status(&self, caller: &str) -> Reply {
        let guard = self.guard.lock().await;
        let filtering = if guard.is_some() {
            match tokio::task::spawn_blocking(moth_engine::guard::active).await {
                Ok(Ok(filtering)) => filtering,
                _ => {
                    return Reply::Failure {
                        code: ErrorCode::TunnelFailed,
                    }
                }
            }
        } else {
            false
        };
        Reply::Guard(moth_ipc::GuardState {
            enabled: guard.is_some(),
            filtering,
            owned_by_caller: guard.as_ref().is_some_and(|p| p.owner == caller),
        })
    }
    pub async fn set_guard(&self, caller: &str, enabled: bool) -> Reply {
        let Ok(_operation) = self.operation.try_lock() else {
            return Reply::Failure {
                code: ErrorCode::Busy,
            };
        };
        let Ok(_maintenance) = moth_ipc::maintenance::TunnelLease::acquire() else {
            return Reply::Failure {
                code: ErrorCode::Busy,
            };
        };
        let mut guard = self.guard.lock().await;
        if guard.as_ref().is_some_and(|g| g.owner != caller) {
            return Reply::Failure {
                code: ErrorCode::NotOwner,
            };
        }
        if enabled {
            let state = self.inner.lock().await;
            if state.owner.as_deref() != Some(caller)
                || state.phase != TunnelPhase::Connected
                || !guard_compatible(&state.policy)
                || state.plan.lan()
            {
                return Reply::Failure {
                    code: ErrorCode::InvalidProfile,
                };
            }
            let (Some(profile), Some(session)) = (&state.profile, &state.session) else {
                return Reply::Failure {
                    code: ErrorCode::TunnelFailed,
                };
            };
            let primary = moth_engine::guard::Endpoint {
                engine: moth_engine::profile_engine(profile),
                host: profile.host.clone(),
                address: session.endpoint().ip(),
                port: session.endpoint().port(),
            };
            let candidates = state.candidates.clone();
            drop(state);
            let mut endpoints = vec![primary];
            // Resolve backups while the established TUN is still active. No
            // unrestricted DNS exception is added when a candidate fails.
            for p in candidates.iter().take(7) {
                if endpoints.iter().any(|e| {
                    e.host == p.host
                        && e.port == p.port
                        && e.engine == moth_engine::profile_engine(p)
                }) {
                    continue;
                }
                if let Ok(Ok(addresses)) = tokio::time::timeout(
                    Duration::from_secs(3),
                    tokio::net::lookup_host((p.host.as_str(), p.port)),
                )
                .await
                {
                    for address in addresses.take(3) {
                        endpoints.push(moth_engine::guard::Endpoint {
                            engine: moth_engine::profile_engine(p),
                            host: p.host.clone(),
                            address: address.ip(),
                            port: address.port(),
                        });
                    }
                }
            }
            let Ok(policy) = moth_engine::guard::Policy::new(caller.into(), endpoints) else {
                return Reply::Failure {
                    code: ErrorCode::InvalidProfile,
                };
            };
            let copy = policy.clone();
            let outcome =
                tokio::task::spawn_blocking(move || moth_engine::guard::enable(&copy)).await;
            *guard = Some(policy);
            if !matches!(outcome, Ok(Ok(()))) {
                return Reply::Failure {
                    code: ErrorCode::TunnelFailed,
                };
            }
        } else {
            if !matches!(
                tokio::task::spawn_blocking(moth_engine::guard::release).await,
                Ok(Ok(()))
            ) {
                return Reply::Failure {
                    code: ErrorCode::TunnelFailed,
                };
            }
            *guard = None;
        }
        drop(guard);
        self.guard_status(caller).await
    }
    pub async fn active_profile(&self, caller: &str) -> Reply {
        let state = self.inner.lock().await;
        Reply::ActiveProfile(if state.owner.as_deref() == Some(caller) {
            state.profile.as_ref().map(|p| p.summary())
        } else {
            None
        })
    }
    pub async fn subscription_proxy(&self, caller: &str) -> Reply {
        let mut state = self.inner.lock().await;
        if state.owner.as_deref() != Some(caller) || state.phase != TunnelPhase::Connected {
            return Reply::Failure {
                code: ErrorCode::NotOwner,
            };
        }
        match state.session.as_mut() {
            Some(session) => {
                if session.alive() {
                    Reply::SubscriptionProxy(session.subscription_proxy())
                } else {
                    Reply::Failure {
                        code: ErrorCode::TunnelFailed,
                    }
                }
            }
            _ => Reply::Failure {
                code: ErrorCode::TunnelFailed,
            },
        }
    }
    pub async fn status(&self, caller: &str) -> Reply {
        let state = self.inner.lock().await;
        let legacy_proxy = state.plan.mode == moth_core::ConnectionMode::Proxy
            && state.phase != TunnelPhase::Disconnected;
        Reply::Tunnel(TunnelState {
            phase: if legacy_proxy {
                TunnelPhase::Failed
            } else {
                state.phase
            },
            owned_by_caller: state.owner.as_deref() == Some(caller),
            error: if legacy_proxy {
                Some(ErrorCode::Unsupported)
            } else {
                state.error
            },
        })
    }
    pub async fn connect(
        &self,
        caller: &str,
        profile: &moth_core::Profile,
        engines: &moth_engine::EngineSet,
    ) -> Reply {
        self.connect_with_policy(
            caller,
            profile,
            engines,
            &moth_core::ConnectionSettings::default(),
        )
        .await
    }
    pub async fn connect_with_policy(
        &self,
        caller: &str,
        profile: &moth_core::Profile,
        engines: &moth_engine::EngineSet,
        policy: &moth_core::ConnectionSettings,
    ) -> Reply {
        self.connect_with_candidates(caller, profile, engines, policy, &[])
            .await
    }
    pub async fn connect_with_candidates(
        &self,
        caller: &str,
        profile: &moth_core::Profile,
        engines: &moth_engine::EngineSet,
        policy: &moth_core::ConnectionSettings,
        candidates: &[moth_core::Profile],
    ) -> Reply {
        self.connect_plan(
            caller,
            profile,
            engines,
            policy,
            candidates,
            moth_engine::SessionPlan::default(),
        )
        .await
    }
    pub async fn connect_plan(
        &self,
        caller: &str,
        profile: &moth_core::Profile,
        engines: &moth_engine::EngineSet,
        policy: &moth_core::ConnectionSettings,
        candidates: &[moth_core::Profile],
        plan: moth_engine::SessionPlan,
    ) -> Reply {
        {
            let guard = self.guard.lock().await;
            if guard.as_ref().is_some_and(|g| g.owner != caller) {
                return Reply::Failure {
                    code: ErrorCode::NotOwner,
                };
            }
            if guard.is_some() && (!guard_compatible(policy) || plan.lan()) {
                return Reply::Failure {
                    code: ErrorCode::InvalidProfile,
                };
            }
        }
        let expected = {
            let state = self.inner.lock().await;
            if state.owner.as_deref().is_some_and(|owner| owner != caller) {
                return Reply::Failure {
                    code: ErrorCode::NotOwner,
                };
            }
            self.generation.fetch_add(1, Ordering::SeqCst) + 1
        };
        self.connect_expected(
            caller,
            profile,
            engines,
            policy,
            candidates,
            ConnectAttempt {
                generation: expected,
                automatic: false,
                plan,
            },
        )
        .await
    }
    async fn connect_expected(
        &self,
        caller: &str,
        profile: &moth_core::Profile,
        engines: &moth_engine::EngineSet,
        policy: &moth_core::ConnectionSettings,
        candidates: &[moth_core::Profile],
        attempt: ConnectAttempt,
    ) -> Reply {
        let expected = attempt.generation;
        let automatic = attempt.automatic;
        let Ok(_guard) = self.operation.try_lock() else {
            return Reply::Failure {
                code: ErrorCode::Busy,
            };
        };
        // Keep the same kernel lease throughout startup. The engine retains a
        // second shared lease for the established session; an exclusive updater
        // cannot race a new Connect or interrupt another user's active tunnel.
        let Ok(_maintenance) = moth_ipc::maintenance::TunnelLease::acquire() else {
            return Reply::Failure {
                code: ErrorCode::Busy,
            };
        };
        let (cancel, mut cancelled) = watch::channel(false);
        {
            let mut state = self.inner.lock().await;
            if !matches!(state.phase, TunnelPhase::Disconnected | TunnelPhase::Failed) {
                return Reply::Failure {
                    code: if state.owner.as_deref() == Some(caller) {
                        ErrorCode::Busy
                    } else {
                        ErrorCode::NotOwner
                    },
                };
            }
            if self.generation.load(Ordering::SeqCst) != expected {
                return Reply::Failure {
                    code: ErrorCode::Busy,
                };
            }
            let failures = if automatic { state.failures } else { 0 };
            let mut backups = candidates.to_vec();
            if !backups.iter().any(|p| p.id == profile.id) {
                backups.push(profile.clone());
            }
            *state = Connection {
                plan: attempt.plan.clone(),
                candidates: backups,
                profile: Some(profile.clone()),
                policy: policy.clone(),
                failures,
                phase: TunnelPhase::Connecting,
                owner: Some(caller.into()),
                cancel: Some(cancel),
                ..Default::default()
            };
        }
        let result = tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(25), engines.connect_session(profile,policy,&attempt.plan)) => result.unwrap_or(Err(moth_engine::EngineError::Timeout)),
            _ = cancelled.changed() => Err(moth_engine::EngineError::Process),
        };
        let startup_error = match &result {
            Err(moth_engine::EngineError::PortUnavailable) => ErrorCode::PortUnavailable,
            _ => ErrorCode::TunnelFailed,
        };
        let result = result.ok();
        let result = match result {
            Some(session) if *cancelled.borrow() => {
                session.stop().await;
                None
            }
            result => result,
        };
        #[cfg(target_os = "linux")]
        if result.is_none() {
            // A cancelled engine future drops kill-on-drop children. Wait for
            // their link to disappear before recovering the exact rule journal;
            // no new Connect can race this operation guard.
            let _ = tokio::task::spawn_blocking(|| {
                let deadline = std::time::Instant::now() + Duration::from_secs(3);
                loop {
                    if moth_engine::recover_network().is_ok()
                        || std::time::Instant::now() >= deadline
                    {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            })
            .await;
        }
        let mut state = self.inner.lock().await;
        state.cancel = None;
        match result {
            Some(session)
                if !*cancelled.borrow() && self.generation.load(Ordering::SeqCst) == expected =>
            {
                state.phase = TunnelPhase::Connected;
                state.session = Some(session);
                state.failures = 0;
                state.next_health = Some(
                    Instant::now()
                        + Duration::from_secs(if policy.health_enabled && !policy.auto_reconnect {
                            u64::from(policy.health_interval_seconds)
                        } else {
                            15
                        }),
                );
            }
            session => {
                // On simultaneous completion/cancellation, teardown still happens.
                drop(session);
                state.phase = TunnelPhase::Failed;
                state.error = Some(startup_error);
                state.failures = state.failures.saturating_add(1);
                state.next_attempt = Some(
                    Instant::now()
                        + Duration::from_secs(
                            (5u64.saturating_mul(1u64 << state.failures.min(4))).min(60),
                        ),
                );
                if !policy.auto_reconnect || self.generation.load(Ordering::SeqCst) != expected {
                    state.owner = None;
                    state.profile = None;
                }
            }
        }
        drop(state);
        self.status(caller).await
    }
    pub async fn disconnect(&self, caller: &str) -> Reply {
        if self
            .guard
            .lock()
            .await
            .as_ref()
            .is_some_and(|g| g.owner != caller)
        {
            return Reply::Failure {
                code: ErrorCode::NotOwner,
            };
        }
        {
            let mut state = self.inner.lock().await;
            if state.owner.as_deref().is_some_and(|owner| owner != caller) {
                return Reply::Failure {
                    code: ErrorCode::NotOwner,
                };
            }
            self.generation.fetch_add(1, Ordering::SeqCst);
            state.profile = None;
            if let Some(cancel) = &state.cancel {
                let _ = cancel.send(true);
            }
            state.phase = TunnelPhase::Disconnecting;
        }
        let _guard = self.operation.lock().await;
        let session = self.inner.lock().await.session.take();
        if let Some(session) = session {
            session.stop().await;
        }
        *self.inner.lock().await = Connection::default();
        self.status(caller).await
    }
    pub async fn monitor(&self, engines: &moth_engine::EngineSet) {
        let Ok(guard) = self.operation.try_lock() else {
            return;
        };
        let expected = self.generation.load(Ordering::SeqCst);
        let mut state = self.inner.lock().await;
        let dead = state
            .session
            .as_mut()
            .is_some_and(|session| !session.alive());
        if dead {
            let session = state.session.take();
            state.phase = TunnelPhase::Disconnecting;
            drop(state);
            if let Some(session) = session {
                session.stop().await;
            }
            state = self.inner.lock().await;
            if self.generation.load(Ordering::SeqCst) != expected {
                return;
            }
            state.phase = TunnelPhase::Failed;
            state.error = Some(ErrorCode::TunnelFailed);
            state.next_attempt = Some(Instant::now() + Duration::from_secs(5));
            if !state.policy.auto_reconnect {
                state.owner = None;
                state.profile = None;
            }
        }
        let check = state.phase == TunnelPhase::Connected
            && (state.policy.health_enabled || state.policy.auto_reconnect)
            && state.next_health.is_some_and(|t| Instant::now() >= t);
        if check {
            let proxy = state
                .session
                .as_ref()
                .map(|session| session.subscription_proxy());
            let url = state.policy.health_url.clone();
            state.next_health = Some(
                Instant::now()
                    + Duration::from_secs(
                        if state.policy.health_enabled && !state.policy.auto_reconnect {
                            u64::from(state.policy.health_interval_seconds)
                        } else {
                            15
                        },
                    ),
            );
            drop(state);
            let healthy = if let Some(proxy) = proxy {
                moth_engine::health_through_proxy(&proxy, &url)
                    .await
                    .is_ok()
            } else {
                false
            };
            state = self.inner.lock().await;
            if self.generation.load(Ordering::SeqCst) != expected
                || state.phase != TunnelPhase::Connected
            {
                return;
            }
            if healthy {
                state.failures = 0;
                state.error = None;
            } else {
                state.failures = state.failures.saturating_add(1);
                state.error = Some(ErrorCode::ProbeFailed);
            }
            if state.failures >= 3 {
                let session = state.session.take();
                state.phase = TunnelPhase::Disconnecting;
                drop(state);
                if let Some(session) = session {
                    session.stop().await;
                }
                state = self.inner.lock().await;
                if self.generation.load(Ordering::SeqCst) != expected {
                    return;
                }
                state.phase = TunnelPhase::Failed;
                state.error = Some(ErrorCode::TunnelFailed);
                state.next_attempt = Some(Instant::now());
                if !state.policy.auto_reconnect {
                    state.owner = None;
                    state.profile = None;
                }
            }
        }
        let retry = if state.phase == TunnelPhase::Failed
            && state.policy.auto_reconnect
            && state.next_attempt.is_some_and(|t| Instant::now() >= t)
        {
            state
                .owner
                .clone()
                .zip(state.profile.clone())
                .map(|(owner, profile)| {
                    (
                        owner,
                        profile,
                        state.policy.clone(),
                        state.candidates.clone(),
                        state.failures,
                        state.plan.clone(),
                    )
                })
        } else {
            None
        };
        drop(state);
        if let Some((owner, mut profile, policy, candidates, failures, plan)) = retry {
            if policy.failover && failures >= 3 {
                let mut fastest = None;
                for candidate in candidates.iter().filter(|p| p.id != profile.id) {
                    if self.generation.load(Ordering::SeqCst) != expected {
                        return;
                    }
                    if let Ok(Ok(measured)) =
                        tokio::time::timeout(Duration::from_secs(15), engines.probe(candidate))
                            .await
                    {
                        if fastest
                            .as_ref()
                            .is_none_or(|(ms, _): &(u64, moth_core::Profile)| {
                                measured.latency_ms < *ms
                            })
                        {
                            fastest = Some((measured.latency_ms, candidate.clone()));
                        }
                    }
                }
                if let Some((_, candidate)) = fastest {
                    profile = candidate;
                }
            }
            drop(guard);
            let _ = self
                .connect_expected(
                    &owner,
                    &profile,
                    engines,
                    &policy,
                    &candidates,
                    ConnectAttempt {
                        generation: expected,
                        automatic: true,
                        plan,
                    },
                )
                .await;
        }
    }
    pub async fn shutdown(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
        let session = self.inner.lock().await.session.take();
        if let Some(session) = session {
            session.stop().await;
        }
    }
}

fn guard_compatible(policy: &moth_core::ConnectionSettings) -> bool {
    policy.split_apps.is_empty()
        && !policy
            .rules
            .iter()
            .any(|r| r.enabled && r.target == moth_core::RuleTarget::Direct)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn proxy_mode_never_reports_legacy_vpn_or_exposes_credentials_to_another_owner() {
        let controller = Controller::default();
        {
            let mut state = controller.inner.lock().await;
            state.phase = TunnelPhase::Connected;
            state.owner = Some("owner".into());
            state.plan = moth_engine::SessionPlan::new(
                moth_core::ConnectionMode::Proxy,
                Some(Default::default()),
            )
            .unwrap();
        }
        assert!(matches!(
            controller.status("owner").await,
            Reply::Tunnel(TunnelState {
                phase: TunnelPhase::Failed,
                error: Some(ErrorCode::Unsupported),
                owned_by_caller: true
            })
        ));
        assert!(matches!(
            controller.connection_status("owner").await,
            Reply::Connection(moth_ipc::ConnectionState {
                mode: moth_core::ConnectionMode::Proxy,
                phase: TunnelPhase::Connected,
                ..
            })
        ));
        assert!(matches!(
            controller.proxy_access("other").await,
            Reply::Failure {
                code: ErrorCode::NotOwner
            }
        ));
        assert!(matches!(
            controller.native_route("owner").await,
            Reply::NativeRoute(moth_ipc::NativeRoute::Blocked)
        ));
        assert!(matches!(
            controller.native_route("other").await,
            Reply::NativeRoute(moth_ipc::NativeRoute::Blocked)
        ));
        controller.disconnect("owner").await;
        assert!(matches!(
            controller.native_route("owner").await,
            Reply::NativeRoute(moth_ipc::NativeRoute::Direct)
        ));
    }
    #[test]
    fn guard_rejects_all_direct_bypass_policies() {
        let mut p = moth_core::ConnectionSettings::default();
        assert!(guard_compatible(&p));
        p.split_apps.push("/usr/bin/curl".into());
        assert!(!guard_compatible(&p));
        p.split_apps.clear();
        p.rules.push(moth_core::RoutingRule {
            id: uuid::Uuid::new_v4(),
            name: "rule".into(),
            enabled: true,
            kind: moth_core::RuleKind::Suffix,
            value: "example.com".into(),
            target: moth_core::RuleTarget::Direct,
        });
        assert!(!guard_compatible(&p));
        p.rules[0].enabled = false;
        assert!(guard_compatible(&p));
    }
    #[tokio::test]
    async fn manual_stop_cancels_pending_recovery_and_other_accounts_cannot_obtain_proxy() {
        let controller = Controller::default();
        {
            let mut state = controller.inner.lock().await;
            state.owner = Some("owner".into());
            state.phase = TunnelPhase::Failed;
            state.policy.auto_reconnect = true;
            state.next_attempt = Some(Instant::now());
            state.profile = Some(
                moth_core::import_links("trojan://synthetic@example.com:443")
                    .unwrap()
                    .remove(0),
            );
        }
        assert!(matches!(
            controller.subscription_proxy("other").await,
            Reply::Failure {
                code: ErrorCode::NotOwner
            }
        ));
        assert!(matches!(
            controller.active_profile("other").await,
            Reply::ActiveProfile(None)
        ));
        let generation = controller.generation.load(Ordering::SeqCst);
        assert!(matches!(
            controller.disconnect("other").await,
            Reply::Failure {
                code: ErrorCode::NotOwner
            }
        ));
        assert!(controller.generation.load(Ordering::SeqCst) == generation);
        controller.disconnect("owner").await;
        assert!(controller.generation.load(Ordering::SeqCst) > generation);
        let state = controller.inner.lock().await;
        assert!(state.profile.is_none() && state.owner.is_none());
    }
    #[tokio::test]
    async fn owner_is_authenticated_sid_and_survives_new_pipe_requests() {
        let controller = Controller::default();
        {
            let mut state = controller.inner.lock().await;
            state.owner = Some("owner".into());
            state.phase = TunnelPhase::Connecting;
        }
        assert!(matches!(
            controller.disconnect("other").await,
            Reply::Failure {
                code: ErrorCode::NotOwner
            }
        ));
        assert!(matches!(
            controller.status("owner").await,
            Reply::Tunnel(TunnelState {
                owned_by_caller: true,
                ..
            })
        ));
        assert!(matches!(
            controller.status("other").await,
            Reply::Tunnel(TunnelState {
                owned_by_caller: false,
                ..
            })
        ));
        assert!(matches!(
            controller.disconnect("owner").await,
            Reply::Tunnel(TunnelState {
                phase: TunnelPhase::Disconnected,
                ..
            })
        ));
    }
}
