use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

pub const IPC_VERSION: u16 = 1;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AppState {
    Disabled,
    Starting,
    Ready,
    Discovering,
    Sending,
    Receiving,
    WaitingForApproval,
    HardwareUnsupported,
    BackendMissing,
    RadioUnavailable,
    BluetoothUnavailable,
    Error,
}

impl fmt::Display for AppState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = serde_json::to_value(self).map_err(|_| fmt::Error)?;
        formatter.write_str(value.as_str().ok_or(fmt::Error)?)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityLevel {
    Unknown,
    Unsupported,
    Candidate,
    SupportedExperimental,
    Supported,
    Preferred,
}

impl CapabilityLevel {
    #[must_use]
    pub const fn usable(self) -> bool {
        matches!(
            self,
            Self::SupportedExperimental | Self::Supported | Self::Preferred
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AdapterBus {
    Pci,
    Usb,
    Platform,
    Virtual,
    Unknown,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RadioCapabilities {
    pub monitor_mode: Option<bool>,
    pub five_ghz: Option<bool>,
    pub action_frame_injection: Option<bool>,
    pub data_frame_injection: Option<bool>,
    pub hardware_timestamps: Option<bool>,
    pub concurrent_station_monitor: Option<bool>,
    pub native_awdl: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HardwareAdapter {
    pub id: String,
    pub interface: Option<String>,
    pub phy: Option<String>,
    pub driver: Option<String>,
    pub bus: AdapterBus,
    pub vendor_id: Option<String>,
    pub device_id: Option<String>,
    pub built_in: bool,
    pub level: CapabilityLevel,
    pub capabilities: RadioCapabilities,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BluetoothStatus {
    pub available: bool,
    pub adapters: Vec<String>,
    pub le_advertising: Option<bool>,
    pub reason: Option<String>,
}

impl Default for BluetoothStatus {
    fn default() -> Self {
        Self {
            available: false,
            adapters: Vec::new(),
            le_advertising: None,
            reason: Some("No Bluetooth controller was detected".to_owned()),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeviceClass {
    Iphone,
    Ipad,
    Mac,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Peer {
    pub id: String,
    pub display_name: Option<String>,
    pub device_class: DeviceClass,
    pub last_seen_millis: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransferDirection {
    Send,
    Receive,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TransferState {
    Preparing,
    WaitingForPeer,
    WaitingForLocalApproval,
    WaitingForRemoteApproval,
    Transferring,
    Finalizing,
    Completed,
    Rejected,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TransferFile {
    pub name: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transfer {
    pub id: String,
    pub direction: TransferDirection,
    pub peer_id: String,
    pub peer_name: Option<String>,
    pub files: Vec<TransferFile>,
    pub bytes_total: Option<u64>,
    pub bytes_transferred: u64,
    pub state: TransferState,
    pub started_at_millis: u64,
    pub error_code: Option<String>,
}

impl Transfer {
    #[must_use]
    pub const fn terminal(&self) -> bool {
        matches!(
            self.state,
            TransferState::Completed
                | TransferState::Rejected
                | TransferState::Cancelled
                | TransferState::Failed
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiscoverabilityLease {
    pub started_at_millis: u64,
    pub expires_at_millis: Option<u64>,
    pub accepting_new_transfers: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatusSnapshot {
    pub version: u16,
    pub state: AppState,
    pub discoverability: Option<DiscoverabilityLease>,
    pub peers: Vec<Peer>,
    pub transfers: Vec<Transfer>,
    pub adapters: Vec<HardwareAdapter>,
    pub selected_adapter: Option<String>,
    pub bluetooth: BluetoothStatus,
    pub message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct AppModel {
    snapshot: StatusSnapshot,
}

impl AppModel {
    #[must_use]
    pub fn new(adapters: Vec<HardwareAdapter>, bluetooth: BluetoothStatus) -> Self {
        let usable = adapters.iter().any(|adapter| adapter.level.usable());
        let state = if usable {
            AppState::Disabled
        } else {
            AppState::HardwareUnsupported
        };
        let message = (!usable).then(|| {
            "No adapter has passed an AWDL compatibility test; connect a validated USB adapter"
                .to_owned()
        });
        Self {
            snapshot: StatusSnapshot {
                version: IPC_VERSION,
                state,
                discoverability: None,
                peers: Vec::new(),
                transfers: Vec::new(),
                adapters,
                selected_adapter: None,
                bluetooth,
                message,
            },
        }
    }

    #[must_use]
    pub fn snapshot(&self) -> StatusSnapshot {
        self.snapshot.clone()
    }

    #[must_use]
    pub fn has_usable_adapter(&self) -> bool {
        self.snapshot
            .adapters
            .iter()
            .any(|adapter| adapter.level.usable())
    }

    pub fn replace_hardware(&mut self, adapters: Vec<HardwareAdapter>, bluetooth: BluetoothStatus) {
        self.snapshot.adapters = adapters;
        self.snapshot.bluetooth = bluetooth;
        if self.snapshot.discoverability.is_none() {
            if self.has_usable_adapter() {
                self.snapshot.state = AppState::Disabled;
                self.snapshot.message = None;
            } else {
                self.snapshot.state = AppState::HardwareUnsupported;
                self.snapshot.message = Some(
                    "No adapter has passed an AWDL compatibility test; connect a validated USB adapter"
                        .to_owned(),
                );
            }
        }
    }

    pub fn select_adapter_by_interface(&mut self, interface: &str) {
        self.snapshot.selected_adapter = self
            .snapshot
            .adapters
            .iter()
            .find(|adapter| adapter.interface.as_deref() == Some(interface))
            .map(|adapter| adapter.id.clone());
    }

    pub fn begin_discovery(&mut self) -> Result<(), StateError> {
        if !self.has_usable_adapter() {
            return Err(StateError::HardwareUnsupported);
        }
        self.snapshot.state = AppState::Discovering;
        self.snapshot.message = Some("Looking for nearby AirDrop receivers".to_owned());
        Ok(())
    }

    pub fn finish_discovery(&mut self, peers: Vec<Peer>, error: Option<String>) {
        self.snapshot.peers = peers;
        self.snapshot.state = if self.snapshot.discoverability.is_some() {
            AppState::Ready
        } else {
            AppState::Disabled
        };
        self.snapshot.message = error;
    }

    pub fn begin_send(&mut self, transfer: Transfer) -> Result<(), StateError> {
        if !self.has_usable_adapter() {
            return Err(StateError::HardwareUnsupported);
        }
        if self
            .snapshot
            .transfers
            .iter()
            .any(|existing| !existing.terminal())
        {
            return Err(StateError::TransferInProgress);
        }
        self.snapshot.transfers.push(transfer);
        self.snapshot.state = AppState::Sending;
        self.snapshot.message = None;
        Ok(())
    }

    pub fn finish_send(&mut self, id: &str, bytes: u64, error: Option<String>) {
        if let Some(transfer) = self
            .snapshot
            .transfers
            .iter_mut()
            .find(|transfer| transfer.id == id)
        {
            transfer.bytes_transferred = bytes.min(transfer.bytes_total.unwrap_or(bytes));
            transfer.state = if error.is_some() {
                TransferState::Failed
            } else {
                TransferState::Completed
            };
            transfer.error_code = error.as_ref().map(|_| "SEND_FAILED".to_owned());
        }
        self.snapshot.state = if self.snapshot.discoverability.is_some() {
            AppState::Ready
        } else {
            AppState::Disabled
        };
        self.snapshot.message = error;
    }

    pub fn enable_receiving(
        &mut self,
        now_millis: u64,
        seconds: Option<u64>,
    ) -> Result<(), StateError> {
        if !self.has_usable_adapter() {
            return Err(StateError::HardwareUnsupported);
        }
        let expires_at_millis = seconds
            .filter(|seconds| *seconds > 0)
            .and_then(|seconds| seconds.checked_mul(1_000))
            .and_then(|duration| now_millis.checked_add(duration));
        self.snapshot.discoverability = Some(DiscoverabilityLease {
            started_at_millis: now_millis,
            expires_at_millis,
            accepting_new_transfers: true,
        });
        self.snapshot.state = AppState::Ready;
        self.snapshot.message = None;
        Ok(())
    }

    pub fn disable_receiving(&mut self) {
        if let Some(lease) = &mut self.snapshot.discoverability {
            lease.accepting_new_transfers = false;
        }
        self.snapshot.discoverability = None;
        self.snapshot.state = if self
            .snapshot
            .transfers
            .iter()
            .any(|transfer| !transfer.terminal())
        {
            AppState::Receiving
        } else if self.has_usable_adapter() {
            AppState::Disabled
        } else {
            AppState::HardwareUnsupported
        };
    }

    pub fn expire(&mut self, now_millis: u64) -> bool {
        let expired = self
            .snapshot
            .discoverability
            .as_ref()
            .and_then(|lease| lease.expires_at_millis)
            .is_some_and(|deadline| deadline <= now_millis);
        if expired {
            self.disable_receiving();
        }
        expired
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum StateError {
    #[error("no Wi-Fi adapter has passed an AWDL compatibility test")]
    HardwareUnsupported,
    #[error("another transfer is already in progress")]
    TransferInProgress,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IpcRequest {
    pub version: u16,
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IpcResponse {
    pub version: u16,
    pub id: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<IpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IpcError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IpcEvent {
    pub version: u16,
    pub event: String,
    pub data: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn adapter(level: CapabilityLevel) -> HardwareAdapter {
        HardwareAdapter {
            id: "phy0".to_owned(),
            interface: Some("wlan0".to_owned()),
            phy: Some("phy0".to_owned()),
            driver: Some("simulated".to_owned()),
            bus: AdapterBus::Virtual,
            vendor_id: None,
            device_id: None,
            built_in: false,
            level,
            capabilities: RadioCapabilities::default(),
            reasons: Vec::new(),
        }
    }

    #[test]
    fn candidate_is_not_silently_usable() {
        let model = AppModel::new(
            vec![adapter(CapabilityLevel::Candidate)],
            BluetoothStatus::default(),
        );
        assert_eq!(model.snapshot().state, AppState::HardwareUnsupported);
    }

    #[test]
    fn discoverability_expires() {
        let mut model = AppModel::new(
            vec![adapter(CapabilityLevel::SupportedExperimental)],
            BluetoothStatus::default(),
        );
        model.enable_receiving(1_000, Some(10)).unwrap();
        assert!(!model.expire(10_999));
        assert!(model.expire(11_000));
        assert_eq!(model.snapshot().state, AppState::Disabled);
    }

    #[test]
    fn indefinite_lease_does_not_expire() {
        let mut model = AppModel::new(
            vec![adapter(CapabilityLevel::Supported)],
            BluetoothStatus::default(),
        );
        model.enable_receiving(1_000, None).unwrap();
        assert!(!model.expire(u64::MAX));
    }

    #[test]
    fn send_lifecycle_is_visible_and_terminal() {
        let mut model = AppModel::new(
            vec![adapter(CapabilityLevel::SupportedExperimental)],
            BluetoothStatus::default(),
        );
        model
            .begin_send(Transfer {
                id: "transfer-1".into(),
                direction: TransferDirection::Send,
                peer_id: "peer-1".into(),
                peer_name: Some("iPhone".into()),
                files: vec![TransferFile {
                    name: "photo.jpg".into(),
                    bytes: 100,
                }],
                bytes_total: Some(100),
                bytes_transferred: 0,
                state: TransferState::WaitingForPeer,
                started_at_millis: 1,
                error_code: None,
            })
            .unwrap();
        assert_eq!(model.snapshot().state, AppState::Sending);
        model.finish_send("transfer-1", 100, None);
        let snapshot = model.snapshot();
        assert_eq!(snapshot.state, AppState::Disabled);
        assert_eq!(snapshot.transfers[0].state, TransferState::Completed);
        assert_eq!(snapshot.transfers[0].bytes_transferred, 100);
    }
}
