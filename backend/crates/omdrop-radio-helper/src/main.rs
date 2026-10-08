use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use omdrop_engine::validate_interface;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const ENGINE: &str = "/usr/lib/omarchy-drop/filin";
const SYSTEMCTL: &str = "/usr/bin/systemctl";
const IW: &str = "/usr/bin/iw";
const IP: &str = "/usr/bin/ip";
const NMCLI: &str = "/usr/bin/nmcli";
const STATE_DIRECTORY: &str = "/run/omarchy-drop";

#[derive(Debug, Parser)]
#[command(
    name = "omdrop-radio",
    about = "Narrow privileged radio boundary for OmarchyDrop"
)]
struct Cli {
    #[command(subcommand)]
    command: RadioCommand,
}

#[derive(Debug, Subcommand)]
enum RadioCommand {
    /// Start the system radio unit for one validated interface.
    Start { adapter: String },
    /// Stop the system radio unit and restore the interface.
    Stop { adapter: String },
    /// Run the pinned AWDL engine (used only by the system unit).
    Run { adapter: String },
    /// Restore the interface state journaled before Run.
    Restore { adapter: String },
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct OriginalState {
    interface_type: String,
    was_up: bool,
    network_manager_managed: Option<bool>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        RadioCommand::Start { adapter } => control_unit("start", &adapter),
        RadioCommand::Stop { adapter } => control_unit("stop", &adapter),
        RadioCommand::Run { adapter } => run_engine(&adapter),
        RadioCommand::Restore { adapter } => restore(&adapter),
    }
}

fn control_unit(verb: &str, adapter: &str) -> Result<()> {
    checked_adapter(adapter)?;
    let unit = unit_name(adapter);
    let status = Command::new(SYSTEMCTL)
        .args([verb, &unit])
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("could not {verb} {unit}"))?;
    if !status.success() {
        bail!("systemctl {verb} {unit} failed with {status}");
    }
    Ok(())
}

fn run_engine(adapter: &str) -> Result<()> {
    checked_adapter(adapter)?;
    if !Path::new(ENGINE).is_file() {
        bail!("packaged AWDL engine is missing: {ENGINE}");
    }
    let state = inspect(adapter)?;
    write_state(adapter, &state)?;

    let error = Command::new(ENGINE)
        .args([
            "-i",
            adapter,
            "-c",
            "44",
            "-h",
            "awdl0",
            "--http-addr",
            "127.0.0.1:9930",
        ])
        .exec();
    Err(error).context("could not execute the packaged AWDL engine")
}

fn restore(adapter: &str) -> Result<()> {
    checked_adapter(adapter)?;
    let path = state_path(adapter);
    let encoded = match fs::read(&path) {
        Ok(encoded) => encoded,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("could not read radio journal {}", path.display()));
        }
    };
    let state: OriginalState =
        serde_json::from_slice(&encoded).context("radio state journal is invalid")?;
    if !allowed_interface_type(&state.interface_type) {
        bail!("refusing to restore unknown interface type");
    }

    run_best_effort(IP, &["link", "set", "dev", adapter, "down"]);
    run_required(IW, &["dev", adapter, "set", "type", &state.interface_type])?;
    if state.was_up {
        run_required(IP, &["link", "set", "dev", adapter, "up"])?;
    }
    if let Some(managed) = state.network_manager_managed {
        run_best_effort(
            NMCLI,
            &[
                "device",
                "set",
                adapter,
                "managed",
                if managed { "yes" } else { "no" },
            ],
        );
    }
    fs::remove_file(&path)
        .with_context(|| format!("could not remove restored journal {}", path.display()))?;
    Ok(())
}

fn inspect(adapter: &str) -> Result<OriginalState> {
    let output = Command::new(IW)
        .args(["dev", adapter, "info"])
        .stdin(Stdio::null())
        .output()
        .context("could not inspect interface type")?;
    if !output.status.success() {
        bail!("iw could not inspect interface {adapter}");
    }
    let info = String::from_utf8_lossy(&output.stdout);
    let interface_type = info
        .lines()
        .find_map(|line| line.trim().strip_prefix("type "))
        .filter(|value| allowed_interface_type(value))
        .context("interface has an unsupported or missing type")?
        .to_owned();
    let flags = fs::read_to_string(format!("/sys/class/net/{adapter}/flags"))
        .context("could not read interface flags")?;
    let was_up = u32::from_str_radix(flags.trim().trim_start_matches("0x"), 16)
        .map(|value| value & 1 != 0)
        .context("interface flags are invalid")?;

    let network_manager_managed = Command::new(NMCLI)
        .args(["-g", "GENERAL.MANAGED", "device", "show", adapter])
        .stdin(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| {
            let value = String::from_utf8_lossy(&output.stdout)
                .trim()
                .to_ascii_lowercase();
            match value.as_str() {
                "yes" => Some(true),
                "no" => Some(false),
                _ => None,
            }
        });

    Ok(OriginalState {
        interface_type,
        was_up,
        network_manager_managed,
    })
}

fn write_state(adapter: &str, state: &OriginalState) -> Result<()> {
    let directory = Path::new(STATE_DIRECTORY);
    fs::create_dir_all(directory).context("could not create radio state directory")?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
        .context("could not secure radio state directory")?;

    let path = state_path(adapter);
    // Preserve the first snapshot across an engine retry. Overwriting it after
    // filin changed the interface to monitor mode would make restoration lie.
    if path.exists() {
        return Ok(());
    }
    let temporary = directory.join(format!(".{adapter}.{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&temporary)
        .context("could not create radio state journal")?;
    let encoded = serde_json::to_vec(state)?;
    file.write_all(&encoded)?;
    file.sync_all()?;
    fs::rename(&temporary, &path).context("could not publish radio state journal")?;
    Ok(())
}

fn run_required(program: &str, args: &[&str]) -> Result<()> {
    let status = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("could not run {program}"))?;
    if !status.success() {
        bail!("{program} failed with {status}");
    }
    Ok(())
}

fn run_best_effort(program: &str, args: &[&str]) {
    let _ = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn checked_adapter(adapter: &str) -> Result<()> {
    validate_interface(adapter).context("invalid adapter name")?;
    if !Path::new("/sys/class/net").join(adapter).exists() {
        bail!("network interface does not exist: {adapter}");
    }
    Ok(())
}

fn state_path(adapter: &str) -> PathBuf {
    Path::new(STATE_DIRECTORY).join(format!("{adapter}.radio.json"))
}

fn unit_name(adapter: &str) -> String {
    format!("omdrop-radio@{adapter}.service")
}

fn allowed_interface_type(value: &str) -> bool {
    matches!(
        value,
        "managed" | "monitor" | "ibss" | "mesh" | "wds" | "AP" | "P2P-client" | "P2P-GO"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unit_name_cannot_be_escaped_by_the_adapter() {
        assert_eq!(unit_name("wlan1"), "omdrop-radio@wlan1.service");
        assert!(validate_interface("wlan1@evil.service").is_err());
        assert!(validate_interface("../wlan1").is_err());
    }

    #[test]
    fn restore_types_are_a_closed_set() {
        assert!(allowed_interface_type("managed"));
        assert!(allowed_interface_type("monitor"));
        assert!(!allowed_interface_type("managed; reboot"));
        assert!(!allowed_interface_type(""));
    }

    #[test]
    fn state_round_trips_without_extra_fields() {
        let state = OriginalState {
            interface_type: "managed".to_owned(),
            was_up: true,
            network_manager_managed: Some(true),
        };
        let encoded = serde_json::to_vec(&state).unwrap();
        assert_eq!(
            serde_json::from_slice::<OriginalState>(&encoded).unwrap(),
            state
        );
    }
}
