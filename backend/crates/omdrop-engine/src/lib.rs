use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;
use tokio::time;

pub const DEFAULT_ENGINE_DIR: &str = "/usr/lib/omarchy-drop";
const PREFLIGHT_TIMEOUT: Duration = Duration::from_secs(15);
const RADIO_CONTROL_TIMEOUT: Duration = Duration::from_secs(45);
const RADIO_HELPER: &str = "/usr/lib/omarchy-drop/omdrop-radio";
const PKEXEC: &str = "/usr/bin/pkexec";
const MAX_SEND_FILES: usize = 64;
const MAX_SEND_BYTES: u64 = 512 * 1_024 * 1_024;

#[derive(Debug, Clone)]
pub struct Engine {
    directory: PathBuf,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct EngineInventory {
    pub directory: PathBuf,
    pub filin_available: bool,
    pub luftlift_available: bool,
    pub radio_helper_available: bool,
    pub polkit_client_available: bool,
    pub awdl_interface_available: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PreflightReport {
    pub adapter: String,
    pub monitor_mode: bool,
    pub action_frame_injection: Option<bool>,
    pub data_frame_injection: Option<bool>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveredPeer {
    pub id: String,
    pub display_name: String,
    pub addresses: Vec<String>,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SendReport {
    pub recipient: String,
    pub file_count: usize,
    pub bytes_sent: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("invalid network interface name")]
    InvalidInterface,
    #[error("the packaged AWDL engine is missing: {0}")]
    Missing(PathBuf),
    #[error("the AWDL adapter preflight timed out")]
    Timeout,
    #[error("could not run the AWDL adapter preflight: {0}")]
    Launch(#[source] std::io::Error),
    #[error("AWDL adapter preflight failed: {0}")]
    PreflightFailed(String),
    #[error("radio control timed out")]
    RadioControlTimeout,
    #[error("radio control failed: {0}")]
    RadioControlFailed(String),
    #[error("the awdl0 link is not running; start a configured radio first")]
    RadioUnavailable,
    #[error("discovery failed: {0}")]
    DiscoveryFailed(String),
    #[error("send request is invalid: {0}")]
    InvalidSend(String),
    #[error("AirDrop send failed: {0}")]
    SendFailed(String),
    #[error("protocol worker stopped unexpectedly")]
    WorkerStopped,
}

impl Default for Engine {
    fn default() -> Self {
        Self::detect()
    }
}

impl Engine {
    #[must_use]
    pub fn detect() -> Self {
        let directory = std::env::var_os("OMDROP_ENGINE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_ENGINE_DIR));
        Self { directory }
    }

    #[must_use]
    pub fn at(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    #[must_use]
    pub fn inventory(&self) -> EngineInventory {
        EngineInventory {
            directory: self.directory.clone(),
            filin_available: executable(&self.filin_path()),
            luftlift_available: executable(&self.luftlift_path()),
            radio_helper_available: executable(Path::new(RADIO_HELPER)),
            polkit_client_available: executable(Path::new(PKEXEC)),
            awdl_interface_available: Path::new("/sys/class/net/awdl0").exists(),
        }
    }

    pub async fn preflight(&self, adapter: &str) -> Result<PreflightReport, EngineError> {
        validate_interface(adapter)?;
        let filin = self.filin_path();
        if !executable(&filin) {
            return Err(EngineError::Missing(filin));
        }

        let mut command = Command::new(&filin);
        command
            .arg("-i")
            .arg(adapter)
            .arg("--check")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let output = time::timeout(PREFLIGHT_TIMEOUT, command.output())
            .await
            .map_err(|_| EngineError::Timeout)?
            .map_err(EngineError::Launch)?;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        let summary = if stdout.is_empty() { stderr } else { stdout };
        if !output.status.success() {
            return Err(EngineError::PreflightFailed(if summary.is_empty() {
                format!("filin exited with {}", output.status)
            } else {
                summary
            }));
        }

        Ok(PreflightReport {
            adapter: adapter.to_owned(),
            monitor_mode: true,
            // filin --check can prove monitor-mode capability, not successful
            // injection. Never upgrade these fields without a packet roundtrip.
            action_frame_injection: None,
            data_frame_injection: None,
            summary,
        })
    }

    pub async fn start_radio(&self, adapter: &str) -> Result<(), EngineError> {
        self.control_radio("start", adapter).await
    }

    pub async fn stop_radio(&self, adapter: &str) -> Result<(), EngineError> {
        self.control_radio("stop", adapter).await
    }

    pub async fn discover(&self, duration: Duration) -> Result<Vec<DiscoveredPeer>, EngineError> {
        require_awdl()?;
        let duration = duration.clamp(Duration::from_secs(1), Duration::from_secs(60));
        let result = tokio::task::spawn_blocking(move || {
            luftlift_rs::mdns::browse("awdl0", duration)
                .map_err(|error| EngineError::DiscoveryFailed(error.to_string()))
        })
        .await
        .map_err(|_| EngineError::WorkerStopped)??;

        let mut seen = HashSet::new();
        Ok(result
            .into_iter()
            .filter(|peer| seen.insert(peer.name.clone()))
            .map(|peer| DiscoveredPeer {
                display_name: display_name(&peer.name),
                id: peer.name,
                addresses: peer
                    .addresses
                    .into_iter()
                    .map(|address| address.to_string())
                    .collect(),
                port: peer.port,
            })
            .collect())
    }

    pub async fn send(
        &self,
        recipient: &str,
        paths: &[PathBuf],
        sender_name: &str,
        transfer_id: &str,
    ) -> Result<SendReport, EngineError> {
        require_awdl()?;
        validate_send_text(recipient, "recipient")?;
        validate_send_text(sender_name, "sender name")?;
        validate_send_text(transfer_id, "transfer ID")?;
        if paths.is_empty() || paths.len() > MAX_SEND_FILES {
            return Err(EngineError::InvalidSend(format!(
                "choose between 1 and {MAX_SEND_FILES} files"
            )));
        }

        let recipient = recipient.to_owned();
        let sender_name = sender_name.to_owned();
        let transfer_id = transfer_id.to_owned();
        let paths = paths.to_vec();
        tokio::task::spawn_blocking(move || {
            send_blocking(&recipient, &paths, &sender_name, &transfer_id)
        })
        .await
        .map_err(|_| EngineError::WorkerStopped)?
    }

    async fn control_radio(&self, verb: &str, adapter: &str) -> Result<(), EngineError> {
        validate_interface(adapter)?;
        for path in [Path::new(PKEXEC), Path::new(RADIO_HELPER)] {
            if !executable(path) {
                return Err(EngineError::Missing(path.to_owned()));
            }
        }
        let mut command = Command::new(PKEXEC);
        command
            .arg(RADIO_HELPER)
            .arg(verb)
            .arg(adapter)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let output = time::timeout(RADIO_CONTROL_TIMEOUT, command.output())
            .await
            .map_err(|_| EngineError::RadioControlTimeout)?
            .map_err(EngineError::Launch)?;
        if output.status.success() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        Err(EngineError::RadioControlFailed(if stderr.is_empty() {
            format!("helper exited with {}", output.status)
        } else {
            stderr.chars().take(1_000).collect()
        }))
    }

    fn filin_path(&self) -> PathBuf {
        self.directory.join("filin")
    }

    fn luftlift_path(&self) -> PathBuf {
        self.directory.join("luftlift")
    }
}

fn send_blocking(
    recipient: &str,
    paths: &[PathBuf],
    sender_name: &str,
    transfer_id: &str,
) -> Result<SendReport, EngineError> {
    use luftlift_rs::client::{send_files, FileToSend, RustlsTransport, SenderConfig};

    let mut total = 0_u64;
    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        let metadata = fs::symlink_metadata(path)
            .map_err(|error| EngineError::InvalidSend(format!("{}: {error}", path.display())))?;
        if !metadata.file_type().is_file() {
            return Err(EngineError::InvalidSend(format!(
                "{} is not a regular file",
                path.display()
            )));
        }
        total = total
            .checked_add(metadata.len())
            .filter(|total| *total <= MAX_SEND_BYTES)
            .ok_or_else(|| {
                EngineError::InvalidSend(format!(
                    "the current safe in-memory sender limit is {} MiB",
                    MAX_SEND_BYTES / 1_024 / 1_024
                ))
            })?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty() && !name.contains(['/', '\\', '\0']))
            .ok_or_else(|| EngineError::InvalidSend("a filename is not portable UTF-8".into()))?
            .to_owned();
        let data = fs::read(path)
            .map_err(|error| EngineError::InvalidSend(format!("{}: {error}", path.display())))?;
        files.push(FileToSend {
            uti_type: uti_for(path).to_owned(),
            name,
            data,
        });
    }

    let peers = luftlift_rs::mdns::browse("awdl0", Duration::from_secs(15))
        .map_err(|error| EngineError::SendFailed(error.to_string()))?;
    let peer = peers
        .iter()
        .find(|peer| peer.name == recipient)
        .ok_or_else(|| {
            EngineError::SendFailed("the selected receiver is no longer nearby".into())
        })?;
    let address = preferred_address(&peer.addresses)
        .ok_or_else(|| EngineError::SendFailed("the receiver has no usable address".into()))?;
    let scope = luftlift_rs::netutil::if_index_for("awdl0");
    let socket = luftlift_rs::netutil::connect_addr_with_scope(address, peer.port, scope);
    let tls = luftlift_rs::tls::build_client_config()
        .map_err(|error| EngineError::SendFailed(error.to_string()))?;
    let mut transport = RustlsTransport::new(Arc::new(tls), socket);
    let sender = SenderConfig::new(sender_name, "LinuxPC", "org.omarchy.omdrop");
    send_files(&mut transport, &sender, &files, transfer_id)
        .map_err(|error| EngineError::SendFailed(error.to_string()))?;
    Ok(SendReport {
        recipient: recipient.to_owned(),
        file_count: files.len(),
        bytes_sent: total,
    })
}

fn require_awdl() -> Result<(), EngineError> {
    if Path::new("/sys/class/net/awdl0").exists() {
        Ok(())
    } else {
        Err(EngineError::RadioUnavailable)
    }
}

fn validate_send_text(value: &str, label: &str) -> Result<(), EngineError> {
    if value.is_empty() || value.len() > 512 || value.contains(['\0', '\n', '\r']) {
        Err(EngineError::InvalidSend(format!("invalid {label}")))
    } else {
        Ok(())
    }
}

fn display_name(service_name: &str) -> String {
    service_name
        .strip_suffix("._airdrop._tcp.local.")
        .unwrap_or(service_name)
        .trim_end_matches('.')
        .to_owned()
}

fn preferred_address(addresses: &[IpAddr]) -> Option<IpAddr> {
    addresses
        .iter()
        .copied()
        .find(|address| matches!(address, IpAddr::V6(value) if value.is_unicast_link_local()))
        .or_else(|| addresses.first().copied())
}

fn uti_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => "public.jpeg",
        Some("png") => "public.png",
        Some("heic") => "public.heic",
        Some("gif") => "com.compuserve.gif",
        Some("mov") => "com.apple.quicktime-movie",
        Some("mp4") => "public.mpeg-4",
        Some("pdf") => "com.adobe.pdf",
        Some("webloc") => "com.apple.web-internet-location",
        _ => "public.data",
    }
}

pub fn validate_interface(value: &str) -> Result<(), EngineError> {
    if value.is_empty()
        || value.len() > 15
        || value.starts_with('-')
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(EngineError::InvalidInterface);
    }
    Ok(())
}

#[cfg(unix)]
fn executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .map(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

    #[test]
    fn rejects_argument_like_or_kernel_invalid_interface_names() {
        for value in [
            "",
            "--help",
            "wlan0;id",
            "wlan0/../../x",
            "interface-name-too-long",
        ] {
            assert!(matches!(
                validate_interface(value),
                Err(EngineError::InvalidInterface)
            ));
        }
        assert!(validate_interface("wlan1").is_ok());
        assert!(validate_interface("wlx00_11-22.3").is_ok());
    }

    #[test]
    fn inventory_requires_executable_regular_files() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        fs::write(temp.path().join("filin"), b"not executable").unwrap();
        fs::write(temp.path().join("luftlift"), b"executable").unwrap();
        fs::set_permissions(
            temp.path().join("luftlift"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        let inventory = Engine::at(temp.path()).inventory();
        assert!(!inventory.filin_available);
        assert!(inventory.luftlift_available);
    }

    #[test]
    fn peer_display_name_removes_only_the_airdrop_suffix() {
        assert_eq!(
            display_name("Alice-iPhone._airdrop._tcp.local."),
            "Alice-iPhone"
        );
        assert_eq!(display_name("literal-device"), "literal-device");
    }

    #[test]
    fn link_local_ipv6_is_preferred_for_awdl() {
        let global = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1));
        let link_local = IpAddr::V6("fe80::1234".parse::<Ipv6Addr>().unwrap());
        assert_eq!(preferred_address(&[global, link_local]), Some(link_local));
        assert_eq!(preferred_address(&[]), None);
    }

    #[test]
    fn common_extensions_map_to_apple_utis() {
        assert_eq!(uti_for(Path::new("photo.JPG")), "public.jpeg");
        assert_eq!(uti_for(Path::new("document.pdf")), "com.adobe.pdf");
        assert_eq!(uti_for(Path::new("archive.bin")), "public.data");
    }
}
