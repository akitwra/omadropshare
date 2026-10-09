use omdrop_core::{
    AdapterBus, BluetoothStatus, CapabilityLevel, HardwareAdapter, RadioCapabilities,
};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[must_use]
pub fn default_socket_path() -> PathBuf {
    if let Some(path) = std::env::var_os("OMDROP_SOCKET") {
        return PathBuf::from(path);
    }
    std::env::var_os("XDG_RUNTIME_DIR").map_or_else(
        || PathBuf::from("/tmp/omarchy-drop-unavailable/omdropd.sock"),
        |runtime| PathBuf::from(runtime).join("omarchy-drop/omdropd.sock"),
    )
}

#[must_use]
pub fn probe_wifi() -> Vec<HardwareAdapter> {
    probe_wifi_at(Path::new("/sys"))
}

#[must_use]
pub fn probe_bluetooth() -> BluetoothStatus {
    probe_bluetooth_at(Path::new("/sys"))
}

#[must_use]
pub fn probe_wifi_at(sys_root: &Path) -> Vec<HardwareAdapter> {
    let phy_root = sys_root.join("class/ieee80211");
    let mut adapters = read_names(&phy_root)
        .unwrap_or_default()
        .into_iter()
        .map(|phy| probe_phy(sys_root, &phy_root.join(&phy), phy))
        .collect::<Vec<_>>();

    if sys_root.join("class/net/awdl0").exists() {
        if let Some(adapter) = adapters.first_mut() {
            adapter.capabilities.native_awdl = true;
            adapter.level = CapabilityLevel::SupportedExperimental;
            adapter.reasons.push(
                "A native awdl0 interface is present; interoperability is still experimental"
                    .to_owned(),
            );
        } else {
            adapters.push(HardwareAdapter {
                id: "native-awdl0".to_owned(),
                interface: Some("awdl0".to_owned()),
                phy: None,
                driver: None,
                bus: AdapterBus::Virtual,
                vendor_id: None,
                device_id: None,
                built_in: true,
                level: CapabilityLevel::SupportedExperimental,
                capabilities: RadioCapabilities {
                    native_awdl: true,
                    ..RadioCapabilities::default()
                },
                reasons: vec!["A native awdl0 interface is present".to_owned()],
            });
        }
    }

    adapters.sort_by(|left, right| left.id.cmp(&right.id));
    adapters
}

fn probe_phy(sys_root: &Path, phy_path: &Path, phy: String) -> HardwareAdapter {
    let device = phy_path.join("device");
    let driver = link_basename(&device.join("driver"));
    let canonical_device = fs::canonicalize(&device).unwrap_or(device.clone());
    let bus = classify_bus(&canonical_device);
    let interfaces = read_names(&device.join("net")).unwrap_or_default();
    let interface = interfaces
        .iter()
        .find(|name| name.as_str() != "awdl0")
        .cloned()
        .or_else(|| interfaces.first().cloned());
    let native_awdl =
        sys_root.join("class/net/awdl0").exists() && interfaces.iter().any(|name| name == "awdl0");
    let (level, mut reasons) = classify_driver(driver.as_deref(), native_awdl);
    reasons.push(
        "Passive kernel metadata cannot prove action/data frame injection; the built-in active test is not available yet"
            .to_owned(),
    );

    HardwareAdapter {
        id: phy.clone(),
        interface,
        phy: Some(phy),
        driver,
        bus,
        vendor_id: read_trimmed(device.join("vendor")),
        device_id: read_trimmed(device.join("device")),
        built_in: bus != AdapterBus::Usb,
        level,
        capabilities: RadioCapabilities {
            native_awdl,
            ..RadioCapabilities::default()
        },
        reasons,
    }
}

#[must_use]
pub fn classify_driver(driver: Option<&str>, native_awdl: bool) -> (CapabilityLevel, Vec<String>) {
    if native_awdl {
        return (
            CapabilityLevel::SupportedExperimental,
            vec!["Kernel exposes a native AWDL interface".to_owned()],
        );
    }

    match driver.unwrap_or_default() {
        "carl9170" => (
            CapabilityLevel::Candidate,
            vec!["carl9170 is known-good in upstream tests, but this adapter has not run the local active test".to_owned()],
        ),
        "mt7921e" | "mt7921u" => (
            CapabilityLevel::Candidate,
            vec!["MediaTek mt76 needs the dual-monitor/exclusive procedure and an active compatibility test".to_owned()],
        ),
        "iwlwifi" => (
            CapabilityLevel::Candidate,
            vec!["Intel support is capability-based and remains experimental until injection is tested".to_owned()],
        ),
        name if name.starts_with("rtw") => (
            CapabilityLevel::Candidate,
            vec!["Realtek monitor mode may inject action frames but drop data frames".to_owned()],
        ),
        "brcmfmac" => (
            CapabilityLevel::Candidate,
            vec!["Generic brcmfmac is not native AWDL; only explicitly validated firmware/driver combinations qualify".to_owned()],
        ),
        "" => (
            CapabilityLevel::Unknown,
            vec!["The kernel driver could not be identified".to_owned()],
        ),
        name => (
            CapabilityLevel::Unknown,
            vec![format!("Driver {name} has no validated AWDL record")],
        ),
    }
}

#[must_use]
pub fn probe_bluetooth_at(sys_root: &Path) -> BluetoothStatus {
    let adapters = read_names(&sys_root.join("class/bluetooth")).unwrap_or_default();
    if adapters.is_empty() {
        BluetoothStatus::default()
    } else {
        BluetoothStatus {
            available: true,
            adapters,
            le_advertising: None,
            reason: Some(
                "A controller exists; BlueZ LE advertising support must be verified over D-Bus"
                    .to_owned(),
            ),
        }
    }
}

fn classify_bus(path: &Path) -> AdapterBus {
    let value = path.to_string_lossy();
    if value.contains("/usb") {
        AdapterBus::Usb
    } else if value.contains("/pci") {
        AdapterBus::Pci
    } else if value.contains("/virtual/") {
        AdapterBus::Virtual
    } else if value.contains("/platform/") {
        AdapterBus::Platform
    } else {
        AdapterBus::Unknown
    }
}

fn read_names(path: &Path) -> io::Result<Vec<String>> {
    let mut names = fs::read_dir(path)?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect::<Vec<_>>();
    names.sort();
    Ok(names)
}

fn link_basename(path: &Path) -> Option<String> {
    fs::canonicalize(path)
        .ok()
        .and_then(|target| target.file_name().map(|name| name.to_owned()))
        .and_then(|name| name.into_string().ok())
}

fn read_trimmed(path: PathBuf) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_tek_is_only_a_candidate() {
        let (level, reasons) = classify_driver(Some("mt7921e"), false);
        assert_eq!(level, CapabilityLevel::Candidate);
        assert!(reasons[0].contains("active compatibility test"));
    }

    #[test]
    fn native_awdl_is_experimental_not_preferred() {
        let (level, _) = classify_driver(Some("brcmfmac"), true);
        assert_eq!(level, CapabilityLevel::SupportedExperimental);
    }

    #[test]
    fn missing_sysfs_is_not_an_error_or_fake_adapter() {
        let temp = tempfile::tempdir().unwrap();
        assert!(probe_wifi_at(temp.path()).is_empty());
        assert!(!probe_bluetooth_at(temp.path()).available);
    }
}
