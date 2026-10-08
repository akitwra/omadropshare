# OmarchyDrop 0.1.0 — Developer Preview

This is the first public developer preview of OmarchyDrop, a native Omarchy top-bar project for AirDrop-compatible sharing.

It establishes the architecture and safety boundaries needed for later hardware interoperability:

- a native Quickshell panel using the current Omarchy plugin contract;
- an unprivileged Rust daemon and `omdropctl` CLI;
- persistent versioned JSONL events over a private Unix socket;
- honest Wi-Fi/Bluetooth capability classification;
- simulated AWDL radio and timing foundations;
- bounded BLE wake serialization;
- safe incoming filename, URL, size, and destination handling;
- systemd/Arch packaging, CI, security documentation, and GPL provenance.

## Important limitation

This preview does **not** transfer files to or from Apple devices yet. The real AWDL backend and AirDrop wire engine are not connected, and hardware support is not claimed without physical interoperability evidence. Unsupported operations fail explicitly instead of presenting a false ready state.

See the [README](https://github.com/akitwra/omadropshare#readme), [hardware policy](https://github.com/akitwra/omadropshare/blob/main/docs/HARDWARE.md), and [protocol status](https://github.com/akitwra/omadropshare/blob/main/docs/PROTOCOL.md) before testing.
