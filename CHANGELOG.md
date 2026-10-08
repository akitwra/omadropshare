# Changelog

All notable changes to OmarchyDrop are documented here.

## Unreleased

### Fixed

- Added root install and uninstall commands that work from an Omarchy-managed plugin checkout.
- Build the backend package from the exact installed plugin commit instead of an unpinned branch tip.
- Start and verify the per-user daemon as part of backend installation.
- Disable makepkg's cross-language LTO injection so Omarchy's `lld` can link
  native `ring` objects reliably.
- Require administrator authentication for active-session radio control instead
  of granting the narrow polkit action automatically.

### Added

- Package the real `filin` AWDL and `luftlift` AirDrop engines from an exact,
  audited GPLv3 commit, outside `PATH` and behind the OmarchyDrop process
  boundary.
- Add a capability-bounded system radio service, narrow polkit helper, and
  crash-safe restoration of interface and NetworkManager state.
- Connect real AirDrop discovery and exact-peer multi-file sending over a
  healthy `awdl0` link.

## [0.1.0] - 2026-10-08

Initial developer preview.

### Added

- Native Omarchy Quickshell top-bar panel and current schema-version-1 plugin manifest.
- Event-driven, versioned JSONL IPC over a private Unix socket.
- Unprivileged Rust daemon and diagnostic/control CLI.
- Explicit application, transfer, discoverability, peer, and hardware state models.
- Conservative Linux Wi-Fi and Bluetooth enumeration with driver-specific diagnostics.
- Simulated AWDL radio backend, TSFT-to-monotonic timing, timestamp-wrap, and availability-window tests.
- Bounded Everyone-mode BLE wake payload model.
- Incoming transfer safety limits, filename validation, URL filtering, and collision-free destination reservation.
- Hardened systemd user service, Arch package recipe, CI, and project documentation.

### Known limitations

- A physical AWDL radio backend is not connected to the daemon.
- The AirDrop mDNS/TLS/HTTP/DVZIP wire engine is not integrated.
- Real iPhone or Mac sending and receiving is therefore intentionally unavailable.
- Active frame-injection compatibility tests, BlueZ registration, privileged radio service, Polkit policy, and crash recovery remain future milestones.
- QML visual validation requires a real Omarchy/Quickshell desktop session.

[0.1.0]: https://github.com/akitwra/omadropshare/releases/tag/v0.1.0
