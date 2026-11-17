use moth_ipc::{EngineInfo, Health, IpcError, Request, Response, PIPE_NAME, PROTOCOL_VERSION};
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{watch, Mutex, Semaphore},
    task::JoinSet,
    time::timeout,
};

pub async fn run(
    development: bool,
    mut shutdown: watch::Receiver<bool>,
    ready: Option<tokio::sync::oneshot::Sender<()>>,
) -> Result<(), IpcError> {
    let root = std::env::current_exe()
        .map_err(|_| IpcError::Unavailable)?
        .parent()
        .ok_or(IpcError::Unavailable)?
        .to_path_buf();
    if !development {
        if moth_ipc::windows::current_sid()? != "S-1-5-18" {
            return Err(IpcError::AccessDenied);
        }
        moth_ipc::windows::validate_installation(&root)?;
    }
    let directory = if development && cfg!(debug_assertions) {
        moth_engine::development_directory().map_err(|_| IpcError::Unavailable)?
    } else {
        root.join("engines")
    };
    let engines = Arc::new(moth_engine::EngineSet::new(directory));
    let status = engines.status();
    if !development && status.iter().any(|s| !s.available) {
        return Err(IpcError::Unavailable);
    }
    let health = Arc::new(Health {
        version: env!("CARGO_PKG_VERSION").into(),
        protocol: PROTOCOL_VERSION,
        system_tunnel: !development,
        engines: status
            .into_iter()
            .map(|e| EngineInfo {
                name: e.name,
                version: e.version,
                available: e.available,
            })
            .collect(),
    });
    let sid = if development {
        Some(moth_ipc::windows::current_sid()?)
    } else {
        None
    };
    let name = if development {
        moth_ipc::windows::development_pipe(std::process::id())
    } else {
        PIPE_NAME.into()
    };
    let mut listener = moth_ipc::windows::create_server(&name, true, sid.as_deref())?;
    if let Some(ready) = ready {
        let _ = ready.send(());
    }
    if development {
        println!("MOTH_READY {}", std::process::id());
    }
    let capacity = Arc::new(Semaphore::new(8));
    let gate = Arc::new(Mutex::new(()));
    let tunnel = Arc::new(crate::tunnel::Controller::default());
    if !development {
        tunnel.restore_guard().await?;
    }
    let mut monitor = tokio::time::interval(Duration::from_millis(500));
    let mut tasks = JoinSet::new();
    loop {
        tokio::select! {
            _=monitor.tick()=>{let tunnel=tunnel.clone();let engines=engines.clone();tasks.spawn(async move{tunnel.monitor(&engines).await;});},
            _=shutdown.changed()=>{if *shutdown.borrow(){break;}},
            accepted=listener.connect()=>{
                accepted.map_err(|_|IpcError::Unavailable)?;
                let connected=listener;
                listener=moth_ipc::windows::create_server(&name,false,sid.as_deref())?;
                let Ok(permit)=capacity.clone().try_acquire_owned() else{drop(connected);continue;};
                let engines=engines.clone();let health=health.clone();let gate=gate.clone();let sid=sid.clone();let tunnel=tunnel.clone();
                tasks.spawn(async move{let _permit=permit;let _=handle(connected,engines,health,gate,sid,tunnel,development).await;});
            },
            Some(_)=tasks.join_next(),if !tasks.is_empty()=>{},
        }
    }
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
    tunnel.shutdown().await;
    Ok(())
}

async fn handle(
    mut pipe: tokio::net::windows::named_pipe::NamedPipeServer,
    engines: Arc<moth_engine::EngineSet>,
    health: Arc<Health>,
    gate: Arc<Mutex<()>>,
    sid: Option<String>,
    tunnel: Arc<crate::tunnel::Controller>,
    development: bool,
) -> Result<(), IpcError> {
    let request: Request = timeout(Duration::from_secs(2), moth_ipc::read_frame(&mut pipe))
        .await
        .map_err(|_| IpcError::Timeout)??;
    let caller = moth_ipc::windows::authorize_client(&pipe, sid.as_deref())?;
    let result = crate::control::dispatch(
        &request,
        &caller,
        &engines,
        &health,
        &gate,
        &tunnel,
        development,
    )
    .await;
    let response = Response {
        version: PROTOCOL_VERSION,
        id: request.id,
        result,
    };
    timeout(
        Duration::from_secs(2),
        moth_ipc::write_frame(&mut pipe, &response),
    )
    .await
    .map_err(|_| IpcError::Timeout)??;
    Ok(())
}
