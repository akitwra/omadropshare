use anyhow::{Context, Result};
use omdrop_core::{
    AppModel, CapabilityLevel, DeviceClass, HardwareAdapter, IpcError, IpcEvent, IpcRequest,
    IpcResponse, Peer, RadioCapabilities, Transfer, TransferDirection, TransferFile, TransferState,
    IPC_VERSION,
};
use omdrop_engine::Engine;
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
    engine: Engine,
}

impl DaemonState {
    #[must_use]
    pub fn new(model: AppModel) -> Self {
        let (events, _) = broadcast::channel(64);
        Self {
            model: Arc::new(RwLock::new(model)),
            events,
            engine: Engine::detect(),
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
        "hardware_test" => {
            let adapter = request.params.get("adapter").and_then(Value::as_str);
            let Some(adapter) = adapter else {
                return error_response(
                    id,
                    "ADAPTER_REQUIRED",
                    "Choose an interface shown by `omdropctl adapters`",
                );
            };
            match state.engine.preflight(adapter).await {
                Ok(report) => success(
                    id,
                    json!({
                        "preflight": report,
                        "validated": false,
                        "message": "Monitor mode is available. Injection still needs an on-air roundtrip, so the adapter was not silently marked compatible."
                    }),
                ),
                Err(error) => error_response(id, "ADAPTER_PREFLIGHT_FAILED", &error.to_string()),
            }
        }
        "radio_start" => {
            let adapter = request.params.get("adapter").and_then(Value::as_str);
            let Some(adapter) = adapter else {
                return error_response(id, "ADAPTER_REQUIRED", "Choose a Wi-Fi interface");
            };
            if let Err(error) = state.engine.start_radio(adapter).await {
                return error_response(id, "RADIO_START_FAILED", &error.to_string());
            }
            let mut ready = false;
            for _ in 0..100 {
                if Path::new("/sys/class/net/awdl0").exists() {
                    ready = true;
                    break;
                }
                time::sleep(Duration::from_millis(100)).await;
            }
            if !ready {
                let _ = state.engine.stop_radio(adapter).await;
                return error_response(
                    id,
                    "RADIO_START_FAILED",
                    "The radio service started, but awdl0 did not appear within 10 seconds; the adapter was restored",
                );
            }
            let adapters = omdrop_platform_linux::probe_wifi();
            let bluetooth = omdrop_platform_linux::probe_bluetooth();
            let mut model = state.model.write().await;
            model.replace_hardware(adapters, bluetooth);
            model.select_adapter_by_interface(adapter);
            drop(model);
            state.publish_state().await;
            success(
                id,
                serde_json::to_value(state.engine.inventory()).unwrap_or(Value::Null),
            )
        }
        "radio_stop" => {
            let adapter = request.params.get("adapter").and_then(Value::as_str);
            let Some(adapter) = adapter else {
                return error_response(id, "ADAPTER_REQUIRED", "Choose a Wi-Fi interface");
            };
            match state.engine.stop_radio(adapter).await {
                Ok(()) => {
                    let adapters = omdrop_platform_linux::probe_wifi();
                    let bluetooth = omdrop_platform_linux::probe_bluetooth();
                    state
                        .model
                        .write()
                        .await
                        .replace_hardware(adapters, bluetooth);
                    state.publish_state().await;
                    success(
                        id,
                        serde_json::to_value(state.engine.inventory()).unwrap_or(Value::Null),
                    )
                }
                Err(error) => error_response(id, "RADIO_STOP_FAILED", &error.to_string()),
            }
        }
        "radio_status" => success(
            id,
            serde_json::to_value(state.engine.inventory()).unwrap_or(Value::Null),
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
                    "engine": state.engine.inventory(),
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
        "discover" => {
            let timeout = request
                .params
                .get("timeout")
                .and_then(Value::as_u64)
                .unwrap_or(15)
                .clamp(1, 60);
            if let Err(error) = state.model.write().await.begin_discovery() {
                return error_response(id, "HARDWARE_UNSUPPORTED", &error.to_string());
            }
            state.publish_state().await;
            match state.engine.discover(Duration::from_secs(timeout)).await {
                Ok(discovered) => {
                    let now = now_millis();
                    let peers = discovered
                        .iter()
                        .map(|peer| Peer {
                            id: peer.id.clone(),
                            display_name: Some(peer.display_name.clone()),
                            device_class: classify_device(&peer.display_name),
                            last_seen_millis: now,
                        })
                        .collect();
                    state.model.write().await.finish_discovery(peers, None);
                    state.publish_state().await;
                    success(id, serde_json::to_value(discovered).unwrap_or(Value::Null))
                }
                Err(error) => {
                    state
                        .model
                        .write()
                        .await
                        .finish_discovery(Vec::new(), Some(error.to_string()));
                    state.publish_state().await;
                    error_response(id, "DISCOVERY_FAILED", &error.to_string())
                }
            }
        }
        "send" => {
            let Some(recipient) = request.params.get("peer").and_then(Value::as_str) else {
                return error_response(id, "PEER_REQUIRED", "Choose a nearby AirDrop receiver");
            };
            let Some(files) = request.params.get("files").and_then(Value::as_array) else {
                return error_response(id, "FILES_REQUIRED", "Choose at least one file");
            };
            let paths = files
                .iter()
                .filter_map(Value::as_str)
                .map(std::path::PathBuf::from)
                .collect::<Vec<_>>();
            if paths.len() != files.len() || paths.is_empty() {
                return error_response(id, "FILES_INVALID", "Every file must be a path string");
            }
            let transfer_id = format!("omdrop-{:016x}-{id:016x}", now_millis());
            let transfer_files = match transfer_files(&paths) {
                Ok(files) => files,
                Err(error) => return error_response(id, "FILES_INVALID", &error.to_string()),
            };
            let Some(total) = transfer_files
                .iter()
                .try_fold(0_u64, |total, file| total.checked_add(file.bytes))
            else {
                return error_response(id, "FILES_INVALID", "Combined file size overflowed");
            };
            let peer_name = state
                .model
                .read()
                .await
                .snapshot()
                .peers
                .iter()
                .find(|peer| peer.id == recipient)
                .and_then(|peer| peer.display_name.clone());
            let transfer = Transfer {
                id: transfer_id.clone(),
                direction: TransferDirection::Send,
                peer_id: recipient.to_owned(),
                peer_name,
                files: transfer_files,
                bytes_total: Some(total),
                bytes_transferred: 0,
                state: TransferState::WaitingForPeer,
                started_at_millis: now_millis(),
                error_code: None,
            };
            if let Err(error) = state.model.write().await.begin_send(transfer) {
                return error_response(id, "SEND_NOT_READY", &error.to_string());
            }
            state.publish_state().await;
            let sender_name = std::env::var("HOSTNAME").unwrap_or_else(|_| "OmarchyDrop".into());
            match state
                .engine
                .send(recipient, &paths, &sender_name, &transfer_id)
                .await
            {
                Ok(report) => {
                    state
                        .model
                        .write()
                        .await
                        .finish_send(&transfer_id, report.bytes_sent, None);
                    state.publish_state().await;
                    success(id, serde_json::to_value(report).unwrap_or(Value::Null))
                }
                Err(error) => {
                    state
                        .model
                        .write()
                        .await
                        .finish_send(&transfer_id, 0, Some(error.to_string()));
                    state.publish_state().await;
                    error_response(id, "SEND_FAILED", &error.to_string())
                }
            }
        }
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

fn classify_device(name: &str) -> DeviceClass {
    let lower = name.to_ascii_lowercase();
    if lower.contains("iphone") {
        DeviceClass::Iphone
    } else if lower.contains("ipad") {
        DeviceClass::Ipad
    } else if lower.contains("mac") {
        DeviceClass::Mac
    } else {
        DeviceClass::Unknown
    }
}

fn transfer_files(paths: &[std::path::PathBuf]) -> std::io::Result<Vec<TransferFile>> {
    paths
        .iter()
        .map(|path| {
            let metadata = std::fs::symlink_metadata(path)?;
            if !metadata.file_type().is_file() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("{} is not a regular file", path.display()),
                ));
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "filename is not portable UTF-8",
                    )
                })?;
            Ok(TransferFile {
                name: name.to_owned(),
                bytes: metadata.len(),
            })
        })
        .collect()
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
