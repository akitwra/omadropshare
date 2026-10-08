# Changelog

All notable changes to OmarchyDrop are documented here.

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
