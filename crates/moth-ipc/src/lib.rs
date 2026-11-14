//! Versioned local control protocol. No shell commands, arbitrary JSON or engine paths.
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use uuid::Uuid;
use zeroize::Zeroizing;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
pub use linux::{service_is_stopped, ServiceClient};
#[cfg(windows)]
pub use windows::{service_is_stopped, ServiceClient};
#[cfg(windows)]
pub mod windows;
pub const PROTOCOL_VERSION: u16 = 2;
pub const MAX_FRAME: usize = 32 * 1024;
pub const SERVICE_NAME: &str = "MothDesktopService";
pub const PIPE_NAME: &str = r"\\.\pipe\Moth.Desktop.Service.v1";

// No Debug on requests: a Probe contains credentials.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u16,
    pub id: Uuid,
    pub command: Command,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Command {
    Status {},
    Probe { link: String },
    Connect { link: String },
    Disconnect {},
    TunnelStatus {},
}
impl Drop for Command {
    fn drop(&mut self) {
        if let Self::Probe { link } | Self::Connect { link } = self {
            use zeroize::Zeroize;
            link.zeroize();
        }
    }
}
impl Request {
    pub fn new(command: Command) -> Self {
        Self {
            version: PROTOCOL_VERSION,
            id: Uuid::new_v4(),
            command,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    pub version: u16,
    pub id: Uuid,
    pub result: Reply,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Reply {
    Status(Health),
    Probe { latency_ms: u64 },
    Tunnel(TunnelState),
    Failure { code: ErrorCode },
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum TunnelPhase {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
    Failed,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TunnelState {
    pub phase: TunnelPhase,
    pub owned_by_caller: bool,
    pub error: Option<ErrorCode>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Health {
    pub version: String,
    pub protocol: u16,
    pub system_tunnel: bool,
    pub engines: Vec<EngineInfo>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineInfo {
    pub name: String,
    pub version: String,
    pub available: bool,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    VersionMismatch,
    InvalidRequest,
    InvalidProfile,
    Busy,
    EngineUnavailable,
    ProbeFailed,
    TunnelFailed,
    NotOwner,
    Unsupported,
}

#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("Служба VPN не установлена или недоступна")]
    Unavailable,
    #[error("Не удалось подтвердить подлинность службы VPN")]
    UntrustedPeer,
    #[error("Нет доступа к службе VPN")]
    AccessDenied,
    #[error("Несовместимая версия службы VPN")]
    VersionMismatch,
    #[error("Служба VPN не ответила вовремя")]
    Timeout,
    #[error("Некорректный ответ службы VPN")]
    Protocol,
}

pub async fn read_frame<T: for<'de> Deserialize<'de>>(
    stream: &mut (impl AsyncRead + Unpin),
) -> Result<T, IpcError> {
    let len = stream.read_u32_le().await.map_err(|_| IpcError::Protocol)? as usize;
    if len == 0 || len > MAX_FRAME {
        return Err(IpcError::Protocol);
    }
    let mut bytes = Zeroizing::new(vec![0u8; len]);
    stream
        .read_exact(&mut bytes)
        .await
        .map_err(|_| IpcError::Protocol)?;
    serde_json::from_slice(&bytes).map_err(|_| IpcError::Protocol)
}
pub async fn write_frame<T: Serialize>(
    stream: &mut (impl AsyncWrite + Unpin),
    value: &T,
) -> Result<(), IpcError> {
    let bytes = Zeroizing::new(serde_json::to_vec(value).map_err(|_| IpcError::Protocol)?);
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err(IpcError::Protocol);
    }
    stream
        .write_u32_le(bytes.len() as u32)
        .await
        .map_err(|_| IpcError::Protocol)?;
    stream
        .write_all(&bytes)
        .await
        .map_err(|_| IpcError::Protocol)?;
    stream.flush().await.map_err(|_| IpcError::Protocol)
}
pub fn validate_response(request: &Request, response: &Response) -> Result<(), IpcError> {
    if response.version != PROTOCOL_VERSION
        || matches!(
            response.result,
            Reply::Failure {
                code: ErrorCode::VersionMismatch
            }
        )
    {
        return Err(IpcError::VersionMismatch);
    }
    if request.id != response.id {
        return Err(IpcError::Protocol);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn bounded_framing_and_secret_free_errors() {
        let (mut writer, mut reader) = tokio::io::duplex(128);
        writer.write_u32_le((MAX_FRAME + 1) as u32).await.unwrap();
        assert!(matches!(
            read_frame::<Request>(&mut reader).await,
            Err(IpcError::Protocol)
        ));
        assert!(serde_json::from_str::<Request>(r#"{"version":1,"id":"00000000-0000-0000-0000-000000000000","command":{"kind":"status","path":"SECRET"}}"#).is_err());
    }
    #[tokio::test]
    async fn fragmented_roundtrip() {
        let request = Request::new(Command::Status {});
        let id = request.id;
        let (mut writer, mut reader) = tokio::io::duplex(8);
        let task = tokio::spawn(async move {
            write_frame(&mut writer, &request).await.unwrap();
        });
        let received: Request = read_frame(&mut reader).await.unwrap();
        assert_eq!(received.id, id);
        task.await.unwrap();
    }
    #[test]
    fn rejects_wrong_correlation_and_version() {
        let request = Request::new(Command::Status {});
        let mut response = Response {
            version: PROTOCOL_VERSION,
            id: Uuid::new_v4(),
            result: Reply::Failure {
                code: ErrorCode::Busy,
            },
        };
        assert!(matches!(
            validate_response(&request, &response),
            Err(IpcError::Protocol)
        ));
        response.id = request.id;
        response.version = PROTOCOL_VERSION + 1;
        assert!(matches!(
            validate_response(&request, &response),
            Err(IpcError::VersionMismatch)
        ));
    }
}
