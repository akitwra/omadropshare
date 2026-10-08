use anyhow::{Context, Result};
use omdrop_core::{
    AppModel, CapabilityLevel, HardwareAdapter, IpcError, IpcEvent, IpcRequest, IpcResponse,
    RadioCapabilities, IPC_VERSION,
};
use serde_json::{json, Value};
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, RwLock};
use tokio::time::{self, Duration};
use tracing::{info, warn};

const MAX_REQUEST_BYTES: u64 = 64 * 1_024;

#[derive(Debug, Clone)]
pub struct DaemonState {
    model: Arc<RwLock<AppModel>>,
    events: broadcast::Sender<IpcEvent>,
}

impl DaemonState {
    #[must_use]
    pub fn new(model: AppModel) -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            model: Arc::new(RwLock::new(model)),
            events,
        }
    }

    async fn state_event(&self) -> IpcEvent {
        let snapshot = self.model.read().await.snapshot();
        IpcEvent {
            version: IPC_VERSION,
            event: "state".to_owned(),
            data: serde_json::to_value(snapshot).unwrap_or(Value::Null),
        }
    }

    async fn publish_state(&self) {
        let _ = self.events.send(self.state_event().await);
    }
}

pub async fn run(socket_path: &Path, mock_hardware: bool) -> Result<()> {
    let adapters = if mock_hardware {
        vec![mock_adapter()]
    } else {
        omdrop_platform_linux::probe_wifi()
    };
    let bluetooth = omdrop_platform_linux::probe_bluetooth();
    let state = DaemonState::new(AppModel::new(adapters, bluetooth));

    prepare_socket(socket_path).await?;
    let listener = UnixListener::bind(socket_path)
        .with_context(|| format!("failed to bind {}", socket_path.display()))?;
    set_socket_permissions(socket_path)?;
    info!(socket = %socket_path.display(), "omdropd listening");

    let ticker_state = state.clone();
    tokio::spawn(async move {
        let mut interval = time::interval(Duration::from_secs(1));
        loop {
            interval.tick().await;
            let expired = ticker_state.model.write().await.expire(now_millis());
            if expired {
                ticker_state.publish_state().await;
            }
        }
    });

    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _) = accepted.context("failed to accept local IPC client")?;
                let client_state = state.clone();
                tokio::spawn(async move {
                    if let Err(error) = handle_client(stream, client_state).await {
                        warn!(%error, "IPC client failed");
                    }
                });
            }
            signal = tokio::signal::ctrl_c() => {
                signal.context("failed to install signal handler")?;
                info!("shutdown requested");
                break;
            }
        }
    }

    let _ = fs::remove_file(socket_path).await;
    Ok(())
}

async fn prepare_socket(socket_path: &Path) -> Result<()> {
    let parent = socket_path.parent().context("socket path has no parent")?;
    fs::create_dir_all(parent)
        .await
        .with_context(|| format!("failed to create {}", parent.display()))?;
    set_directory_permissions(parent)?;
    match fs::symlink_metadata(socket_path).await {
        Ok(metadata) if metadata.file_type().is_socket() => {
            if UnixStream::connect(socket_path).await.is_ok() {
                anyhow::bail!(
                    "another omdropd instance is already listening on {}",
                    socket_path.display()
                );
            }
            fs::remove_file(socket_path).await.with_context(|| {
                format!("failed to remove stale socket {}", socket_path.display())
            })?;
        }
        Ok(_) => anyhow::bail!(
            "refusing to replace non-socket path {}",
            socket_path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("failed to inspect socket path"),
    }
    Ok(())
}

#[cfg(unix)]
fn set_directory_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .context("failed to secure runtime directory")
}

#[cfg(unix)]
fn set_socket_permissions(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .context("failed to secure IPC socket")
}

async fn handle_client(stream: UnixStream, state: DaemonState) -> Result<()> {
    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half).take(MAX_REQUEST_BYTES + 1);
    let mut line = String::new();
    let bytes = reader.read_line(&mut line).await?;
    if bytes == 0 {
        return Ok(());
    }
    if bytes as u64 > MAX_REQUEST_BYTES {
        anyhow::bail!("IPC request exceeded {MAX_REQUEST_BYTES} bytes");
    }
    let request: IpcRequest = serde_json::from_str(&line).context("invalid IPC request")?;
    if request.version != IPC_VERSION {
        write_json_line(
            &mut write_half,
            &error_response(
                request.id,
                "VERSION_MISMATCH",
                "The client and daemon use different IPC versions",
            ),
        )
        .await?;
        return Ok(());
    }

    if request.method == "events" {
        let mut receiver = state.events.subscribe();
        write_json_line(&mut write_half, &state.state_event().await).await?;
        loop {
            match receiver.recv().await {
                Ok(event) => write_json_line(&mut write_half, &event).await?,
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    write_json_line(&mut write_half, &state.state_event().await).await?;
                }
                Err(broadcast::error::RecvError::Closed) => return Ok(()),
            }
        }
    }

    let response = process_request(&state, request).await;
    write_json_line(&mut write_half, &response).await?;
    Ok(())
}

pub async fn process_request(state: &DaemonState, request: IpcRequest) -> IpcResponse {
    let id = request.id;
    match request.method.as_str() {
        "status" | "adapters" => {
            let snapshot = state.model.read().await.snapshot();
            success(id, serde_json::to_value(snapshot).unwrap_or(Value::Null))
        }
        "receive_on" => {
            let seconds = request.params.get("seconds").and_then(Value::as_u64);
            let result = state
                .model
                .write()
                .await
                .enable_receiving(now_millis(), seconds);
            match result {
                Ok(()) => {
                    state.publish_state().await;
                    success(id, json!({"enabled": true}))
                }
                Err(error) => error_response(id, "HARDWARE_UNSUPPORTED", &error.to_string()),
            }
        }
        "receive_off" => {
            state.model.write().await.disable_receiving();
            state.publish_state().await;
            success(id, json!({"enabled": false}))
        }
        "hardware_probe" => {
            let adapters = omdrop_platform_linux::probe_wifi();
            let bluetooth = omdrop_platform_linux::probe_bluetooth();
            state
                .model
                .write()
                .await
                .replace_hardware(adapters, bluetooth);
            state.publish_state().await;
            let snapshot = state.model.read().await.snapshot();
            success(id, serde_json::to_value(snapshot).unwrap_or(Value::Null))
        }
        "hardware_test" => error_response(
            id,
            "ACTIVE_TEST_UNAVAILABLE",
            "The disruptive injection test is not implemented yet; no adapter status was upgraded",
        ),
        "diagnostics" => {
            let snapshot = state.model.read().await.snapshot();
            let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
                .unwrap_or_else(|_| "unknown".to_owned())
                .trim()
                .to_owned();
            success(
                id,
                json!({
                    "application": "OmarchyDrop",
                    "applicationVersion": env!("CARGO_PKG_VERSION"),
                    "ipcVersion": IPC_VERSION,
                    "kernel": kernel,
                    "status": snapshot
                }),
            )
        }
        "recover" => success(
            id,
            json!({
                "restored": false,
                "message": "No radio session journal exists in this implementation"
            }),
        ),
        "peers" => {
            let snapshot = state.model.read().await.snapshot();
            success(
                id,
                serde_json::to_value(snapshot.peers).unwrap_or(Value::Null),
            )
        }
        "discover" | "send" => error_response(
            id,
            "RADIO_BACKEND_UNAVAILABLE",
            "A validated physical AWDL backend is required for this operation",
        ),
        _ => error_response(id, "UNKNOWN_METHOD", "Unknown IPC method"),
    }
}

fn success(id: u64, result: Value) -> IpcResponse {
    IpcResponse {
        version: IPC_VERSION,
        id,
        ok: true,
        result: Some(result),
        error: None,
    }
}

fn error_response(id: u64, code: &str, message: &str) -> IpcResponse {
    IpcResponse {
        version: IPC_VERSION,
        id,
        ok: false,
        result: None,
        error: Some(IpcError {
            code: code.to_owned(),
            message: message.to_owned(),
        }),
    }
}

async fn write_json_line<W, T>(writer: &mut W, value: &T) -> Result<()>
where
    W: AsyncWriteExt + Unpin,
    T: serde::Serialize,
{
    let mut encoded = serde_json::to_vec(value)?;
    encoded.push(b'\n');
    writer.write_all(&encoded).await?;
    writer.flush().await?;
    Ok(())
}

#[must_use]
pub fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

fn mock_adapter() -> HardwareAdapter {
    HardwareAdapter {
        id: "simulated-radio".to_owned(),
        interface: Some("sim0".to_owned()),
        phy: Some("simulated".to_owned()),
        driver: Some("omdrop-simulated".to_owned()),
        bus: omdrop_core::AdapterBus::Virtual,
        vendor_id: None,
        device_id: None,
        built_in: false,
        level: CapabilityLevel::SupportedExperimental,
        capabilities: RadioCapabilities {
            monitor_mode: Some(true),
            five_ghz: Some(true),
            action_frame_injection: Some(true),
            data_frame_injection: Some(true),
            hardware_timestamps: Some(true),
            concurrent_station_monitor: Some(true),
            native_awdl: false,
        },
        reasons: vec!["Simulation only; never shown as real hardware support".to_owned()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omdrop_core::{AppState, BluetoothStatus};

    fn request(method: &str, params: Value) -> IpcRequest {
        IpcRequest {
            version: IPC_VERSION,
            id: 7,
            method: method.to_owned(),
            params,
        }
    }

    #[tokio::test]
    async fn unsupported_hardware_refuses_receive() {
        let state = DaemonState::new(AppModel::new(Vec::new(), BluetoothStatus::default()));
        let response =
            process_request(&state, request("receive_on", json!({"seconds": 600}))).await;
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "HARDWARE_UNSUPPORTED");
    }

    #[tokio::test]
    async fn mock_hardware_supports_bounded_receive() {
        let state = DaemonState::new(AppModel::new(
            vec![mock_adapter()],
            BluetoothStatus::default(),
        ));
        let response =
            process_request(&state, request("receive_on", json!({"seconds": 600}))).await;
        assert!(response.ok);
        assert_eq!(state.model.read().await.snapshot().state, AppState::Ready);
        assert!(state
            .model
            .read()
            .await
            .snapshot()
            .discoverability
            .unwrap()
            .expires_at_millis
            .is_some());
    }
}
