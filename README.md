# OmarchyDrop

OmarchyDrop is a native Omarchy top-bar project for AirDrop-compatible file sharing. Its architecture is designed for both directions, honest hardware detection, a supported USB-radio fallback, an unprivileged Rust daemon, and a minimal future radio service.

> Current status: **0.1.0 developer preview**, not an interoperable AirDrop release. The plugin, daemon, IPC, diagnostics, simulated radio, timing logic, BLE wake serialization, and receive-path safety utilities work and are tested. A physical AWDL backend and AirDrop wire engine are not connected yet, so real iPhone transfers are intentionally refused rather than misrepresented.

OmarchyDrop is independent open-source software. It is not affiliated with or endorsed by Apple. AirDrop is a trademark of Apple Inc.

## What works now

- Current Omarchy schema-version-1 `bar-widget` manifest and native `qs.Ui.Panel` popup.
- One persistent `omdropctl events --jsonl` status stream; no polling process loop and no second Quickshell instance.
- Versioned, mode-0600 Unix-socket IPC between the QML UI, CLI, and unprivileged daemon.
- Explicit application and transfer state models.
- Bounded Everyone-mode receive leases with automatic expiry.
- Passive Linux Wi-Fi/Bluetooth enumeration and conservative driver classification.
- `omdropctl status`, `events`, `peers`, `adapters`, `hardware probe`, `hardware test`, `diagnostics`, `receive`, and `recover` command surfaces.
- Simulated AWDL backend, TSFT/monotonic timing, timestamp wraparound, and availability-window tests.
- Bounded BLE wake payload model based on verified BlueZ D-Bus measurements.
- Safe filename, size/count limit, URL-scheme, and collision-free destination utilities.
- Hardened systemd user service and Arch `PKGBUILD` for the non-root backend.

## Architecture

```text
Omarchy Quickshell panel
        │ JSONL + local actions
        ▼
omdropd (user daemon)
        ├── AirDrop application protocol
        ├── BLE wake coordinator
        └── hardware/session manager
                    │ narrow authenticated API
                    ▼
             radio service
                    │
              AWDL / awdl0
                    │
             iPhone or Mac
```

See [Architecture](docs/ARCHITECTURE.md), [research](docs/RESEARCH.md), [protocol](docs/PROTOCOL.md), [security](docs/SECURITY.md), and [licensing](docs/LICENSING.md).

## Development setup

Install a stable Rust toolchain with `rustfmt` and `clippy`, then:

```bash
cd backend
cargo build --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --locked
```

Run the daemon and inspect this machine:

```bash
backend/target/debug/omdropd
backend/target/debug/omdropctl status
backend/target/debug/omdropctl adapters
backend/target/debug/omdropctl diagnostics --json
```

For UI development without radio hardware, run `omdropd --mock-hardware`. The simulated device is visibly marked as simulation and never upgrades real hardware support.

On a current Omarchy checkout, validate the plugin root with:

```bash
omarchy plugin validate .
```

Do not create a separate Git worktree for cloud tasks; use the checkout already provided under `/workspace`.

## Installation model

The QML repository can eventually be installed with:

```bash
omarchy plugin add https://github.com/akitwra/omadropshare.git --enable
```

That operation only installs plugin code and never silently modifies the system. Build/install the backend explicitly with the Arch package recipe:

```bash
cd packaging
makepkg -si
systemctl --user enable --now omdropd.service
```

The package currently installs only the unprivileged daemon and CLI. It does not install a root radio service, kernel patch, DKMS module, NetworkManager override, or passwordless sudo rule.

## CLI

```bash
omdropctl status
omdropctl status --json
omdropctl events --jsonl
omdropctl peers --json
omdropctl adapters
omdropctl hardware probe
omdropctl hardware test --adapter wlan1 --active
omdropctl receive on --seconds 600
omdropctl receive off
omdropctl diagnostics --json
omdropctl recover
```

Discovery and send commands are present but return `RADIO_BACKEND_UNAVAILABLE` until a validated backend is integrated.

## Hardware policy

OmarchyDrop never equates monitor mode with working AWDL. A device remains `Candidate` until action-frame injection, data-frame injection, timing, channel control, cleanup, and real Apple-device transfers pass. See [the hardware matrix](docs/HARDWARE.md).

The intended fallback is a dedicated validated USB adapter: normal Internet remains on the internal radio while AWDL uses USB. MediaTek, Intel, Broadcom, and Realtek strategies remain separate and capability-driven.

## Sending and receiving roadmap

The UI and daemon are designed for incoming approval, progress, cancellation, history, peer expiry, multi-file sends, portal file picking, notifications, and bounded BLE wake. Real usability requires the remaining protocol/radio milestones and physical tests listed in [PROTOCOL.md](docs/PROTOCOL.md). Contacts Only is outside the first release; the project never asks for an Apple ID password.

## Privacy

There is no telemetry. Nearby-device state is ephemeral, diagnostics redact identity-bearing values by default, BLE wake is bounded, and received URLs are never opened automatically. Packet capture is absent and will remain opt-in and duration-limited if added.

## License

GPL-3.0-only. The project deliberately follows a GPL-compatible derivative strategy for OWL/OpenDrop research; see [LICENSING.md](docs/LICENSING.md) for provenance and the separate GPL-2.0-only native Broadcom boundary.

