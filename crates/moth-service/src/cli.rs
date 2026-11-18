//! Typed user controls; no raw engine config or credential-bearing reply.
use moth_ipc::{Command, Reply};
pub fn handles(option: &str) -> bool {
    matches!(
        option,
        "--guard-status"
            | "--guard-on"
            | "--guard-off"
            | "--traffic"
            | "--connection-status"
            | "--connect-proxy"
    )
}
pub fn run(option: &str) -> Result<(), String> {
    let command = match option {
        "--guard-status" => Command::GuardStatus {},
        "--guard-on" => Command::SetGuard { enabled: true },
        "--guard-off" => Command::SetGuard { enabled: false },
        "--traffic" => Command::Traffic {},
        "--connection-status" => Command::ConnectionStatus {},
        "--connect-proxy" => {
            use std::io::Read;
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "camelCase", deny_unknown_fields)]
            struct Input {
                link: String,
                mode: moth_core::ConnectionMode,
                listener: moth_core::PublicProxyOptions,
            }
            let mut text = zeroize::Zeroizing::new(String::new());
            std::io::stdin()
                .take(32769)
                .read_to_string(&mut text)
                .map_err(|_| "Invalid connection input")?;
            if text.len() > 32768 {
                return Err("Connection input too large".into());
            }
            let input: Input =
                serde_json::from_str(&text).map_err(|_| "Invalid typed connection input")?;
            input.listener.validate().map_err(str::to_owned)?;
            Command::BeginConnection {
                link: input.link,
                mode: input.mode,
                listener: Some(input.listener),
                profile_id: uuid::Uuid::new_v4(),
                candidates: Vec::new(),
                policy: Box::default(),
            }
        }
        _ => return Err("Unsupported control option".into()),
    };
    tokio::runtime::Runtime::new()
        .map_err(|_| "Runtime unavailable")?
        .block_on(async {
            let response = moth_ipc::ServiceClient::production()
                .call(command)
                .await
                .map_err(|e| e.to_string())?;
            if !matches!(
                response.result,
                Reply::Connection(_) | Reply::Guard(_) | Reply::Traffic(_) | Reply::Failure { .. }
            ) {
                return Err("Unexpected service reply".into());
            }
            println!(
                "{}",
                serde_json::to_string(&response.result).map_err(|_| "Serialization failed")?
            );
            Ok(())
        })
}
