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
    session: Option<moth_engine::Tunnel>,
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
    inner: Mutex<Connection>,
    operation: Mutex<()>,
    generation: AtomicU64,
}
struct ConnectAttempt {
    generation: u64,
    automatic: bool,
}
impl Controller {
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
        Reply::Tunnel(TunnelState {
            phase: state.phase,
            owned_by_caller: state.owner.as_deref() == Some(caller),
            error: state.error,
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
            result = tokio::time::timeout(Duration::from_secs(25), engines.connect_with_policy(profile,policy)) => result.ok().and_then(Result::ok),
            _ = cancelled.changed() => None,
        };
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
                state.error = Some(ErrorCode::TunnelFailed);
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
                    )
                })
        } else {
            None
        };
        drop(state);
        if let Some((owner, mut profile, policy, candidates, failures)) = retry {
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

#[cfg(test)]
mod tests {
    use super::*;
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
