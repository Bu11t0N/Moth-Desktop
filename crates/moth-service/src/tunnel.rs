use moth_ipc::{ErrorCode, Reply, TunnelPhase, TunnelState};
use std::time::Duration;
use tokio::sync::{watch, Mutex};

#[derive(Default)]
struct Connection {
    phase: TunnelPhase,
    owner: Option<String>,
    error: Option<ErrorCode>,
    session: Option<moth_engine::Tunnel>,
    cancel: Option<watch::Sender<bool>>,
}
#[derive(Default)]
pub struct Controller {
    inner: Mutex<Connection>,
    operation: Mutex<()>,
}
impl Controller {
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
        let Ok(_guard) = self.operation.try_lock() else {
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
            *state = Connection {
                phase: TunnelPhase::Connecting,
                owner: Some(caller.into()),
                cancel: Some(cancel),
                ..Default::default()
            };
        }
        let result = tokio::select! {
            result = tokio::time::timeout(Duration::from_secs(25), engines.connect(profile)) => result.ok().and_then(Result::ok),
            _ = cancelled.changed() => None,
        };
        let mut state = self.inner.lock().await;
        state.cancel = None;
        match result {
            Some(session) if !*cancelled.borrow() => {
                state.phase = TunnelPhase::Connected;
                state.session = Some(session);
            }
            session => {
                // On simultaneous completion/cancellation, teardown still happens.
                drop(session);
                state.phase = TunnelPhase::Failed;
                state.error = Some(ErrorCode::TunnelFailed);
                state.owner = None;
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
    pub async fn monitor(&self) {
        let mut state = self.inner.lock().await;
        if let Some(session) = &mut state.session {
            if !session.alive() {
                let session = state.session.take();
                state.phase = TunnelPhase::Disconnecting;
                drop(state);
                if let Some(session) = session {
                    session.stop().await;
                }
                let mut state = self.inner.lock().await;
                state.phase = TunnelPhase::Failed;
                state.error = Some(ErrorCode::TunnelFailed);
                state.owner = None;
            }
        }
    }
    pub async fn shutdown(&self) {
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
