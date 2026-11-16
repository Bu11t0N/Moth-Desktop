use moth_ipc::{Command, EngineInfo, ErrorCode, Health, Reply, Request, PROTOCOL_VERSION};
use std::time::Duration;
use tokio::{sync::Mutex, time::timeout};

pub async fn dispatch(
    request: &Request,
    caller: &str,
    engines: &moth_engine::EngineSet,
    health: &Health,
    gate: &Mutex<()>,
    tunnel: &crate::tunnel::Controller,
    development: bool,
) -> Reply {
    if request.version != PROTOCOL_VERSION {
        Reply::Failure {
            code: ErrorCode::VersionMismatch,
        }
    } else {
        match &request.command {
            Command::Status {} => {
                let status = engines.status();
                Reply::Status(Health {
                    version: health.version.clone(),
                    protocol: health.protocol,
                    system_tunnel: health.system_tunnel && status.iter().all(|e| e.available),
                    engines: status
                        .into_iter()
                        .map(|e| EngineInfo {
                            name: e.name,
                            version: e.version,
                            available: e.available,
                        })
                        .collect(),
                })
            }
            Command::TunnelStatus {} => tunnel.status(caller).await,
            Command::SubscriptionProxy {} if !development => {
                tunnel.subscription_proxy(caller).await
            }
            Command::SubscriptionProxy {} => Reply::Failure {
                code: ErrorCode::Unsupported,
            },
            Command::Disconnect {} if !development => tunnel.disconnect(caller).await,
            Command::Connect { link } if !development => match moth_core::import_links(link) {
                Ok(mut profiles) if profiles.len() == 1 => {
                    tunnel.connect(caller, &profiles.remove(0), engines).await
                }
                _ => Reply::Failure {
                    code: ErrorCode::InvalidProfile,
                },
            },
            Command::Connect { .. } | Command::Disconnect {} => Reply::Failure {
                code: ErrorCode::Unsupported,
            },
            Command::Probe { link } => match gate.try_lock() {
                Err(_) => Reply::Failure {
                    code: ErrorCode::Busy,
                },
                Ok(_guard) => {
                    // Probes also execute pinned cores. Keep maintenance from
                    // replacing them while a production probe is in flight.
                    // Read-only development console tests do not create global
                    // installation state on the developer's machine.
                    let _maintenance = if development {
                        None
                    } else {
                        match moth_ipc::maintenance::TunnelLease::acquire() {
                            Ok(lease) => Some(lease),
                            Err(_) => {
                                return Reply::Failure {
                                    code: ErrorCode::Busy,
                                }
                            }
                        }
                    };
                    match moth_core::import_links(link) {
                        Ok(mut profiles) if profiles.len() == 1 => {
                            let profile = profiles.remove(0);
                            match timeout(Duration::from_secs(25), engines.probe(&profile)).await {
                                Ok(Ok(result)) => Reply::Probe {
                                    latency_ms: result.latency_ms,
                                },
                                Ok(Err(
                                    moth_engine::EngineError::Missing
                                    | moth_engine::EngineError::Integrity,
                                )) => Reply::Failure {
                                    code: ErrorCode::EngineUnavailable,
                                },
                                _ => Reply::Failure {
                                    code: ErrorCode::ProbeFailed,
                                },
                            }
                        }
                        _ => Reply::Failure {
                            code: ErrorCode::InvalidProfile,
                        },
                    }
                }
            },
        }
    }
}
