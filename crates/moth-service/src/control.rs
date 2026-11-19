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
            Command::ConnectionStatus {} => tunnel.connection_status(caller).await,
            Command::PolicyGroups {} if !development => tunnel.policy_groups(caller, None).await,
            Command::ChoosePolicyMember { group, member }
                if !development && group.len() <= 160 && member.len() <= 160 =>
            {
                tunnel.policy_groups(caller, Some((group, member))).await
            }
            Command::BeginClashPolicy {
                document,
                settings,
                mode,
                listener,
            } if !development => {
                let policy = moth_core::inspect_clash_policy(document)
                    .ok()
                    .flatten()
                    .and_then(|p| p.policy);
                match (
                    policy,
                    moth_engine::SessionPlan::new(*mode, listener.clone()),
                ) {
                    (Some(policy), Ok(plan)) => {
                        let reply = tunnel
                            .connect_clash(caller, &policy, engines, settings, plan)
                            .await;
                        if matches!(reply, Reply::Tunnel(_)) {
                            tunnel.connection_status(caller).await
                        } else {
                            reply
                        }
                    }
                    _ => Reply::Failure {
                        code: ErrorCode::InvalidProfile,
                    },
                }
            }
            Command::PolicyGroups {}
            | Command::ChoosePolicyMember { .. }
            | Command::BeginClashPolicy { .. } => Reply::Failure {
                code: ErrorCode::Unsupported,
            },
            Command::ProxyAccess {} if !development => tunnel.proxy_access(caller).await,
            Command::NativeRoute {} if !development => tunnel.native_route(caller).await,
            Command::ProxyAccess {} | Command::NativeRoute {} => Reply::Failure {
                code: ErrorCode::Unsupported,
            },
            Command::Traffic {} if !development => tunnel.traffic(caller).await,
            Command::Traffic {} => Reply::Failure {
                code: ErrorCode::Unsupported,
            },
            Command::GuardStatus {} if !development => tunnel.guard_status(caller).await,
            Command::SetGuard { enabled } if !development => {
                tunnel.set_guard(caller, *enabled).await
            }
            Command::GuardStatus {} | Command::SetGuard { .. } => Reply::Failure {
                code: ErrorCode::Unsupported,
            },
            Command::ActiveProfile {} => tunnel.active_profile(caller).await,
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
            Command::ConnectWithPolicy {
                link,
                policy,
                profile_id,
                candidates,
            }
            | Command::BeginConnection {
                link,
                policy,
                profile_id,
                candidates,
                ..
            } if !development && policy.validate().is_ok() && candidates.len() <= 7 => {
                match moth_core::import_links(link) {
                    Ok(mut profiles) if profiles.len() == 1 => {
                        let mut profile = profiles.remove(0);
                        profile.id = *profile_id;
                        let mut backups = Vec::new();
                        let mut ids = std::collections::HashSet::from([*profile_id]);
                        for candidate in candidates {
                            if !ids.insert(candidate.id) {
                                return Reply::Failure {
                                    code: ErrorCode::InvalidProfile,
                                };
                            }
                            let Ok(mut parsed) = moth_core::import_links(&candidate.link) else {
                                return Reply::Failure {
                                    code: ErrorCode::InvalidProfile,
                                };
                            };
                            if parsed.len() != 1 {
                                return Reply::Failure {
                                    code: ErrorCode::InvalidProfile,
                                };
                            }
                            let mut parsed = parsed.remove(0);
                            parsed.id = candidate.id;
                            backups.push(parsed);
                        }
                        if let Command::BeginConnection { mode, listener, .. } = &request.command {
                            let Ok(plan) = moth_engine::SessionPlan::new(*mode, listener.clone())
                            else {
                                return Reply::Failure {
                                    code: ErrorCode::InvalidProfile,
                                };
                            };
                            let result = tunnel
                                .connect_plan(caller, &profile, engines, policy, &backups, plan)
                                .await;
                            if matches!(result, Reply::Tunnel(_)) {
                                tunnel.connection_status(caller).await
                            } else {
                                result
                            }
                        } else {
                            tunnel
                                .connect_with_candidates(
                                    caller, &profile, engines, policy, &backups,
                                )
                                .await
                        }
                    }
                    _ => Reply::Failure {
                        code: ErrorCode::InvalidProfile,
                    },
                }
            }
            Command::ConnectWithPolicy { .. } | Command::BeginConnection { .. } => Reply::Failure {
                code: ErrorCode::Unsupported,
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
