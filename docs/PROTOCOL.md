# Protocol notes

OmarchyDrop separates AirDrop's application protocol from AWDL's link layer. This document describes the target and the currently implemented foundation; it is not an assertion that interoperability is complete.

## Stack

```text
AirDrop HTTP endpoints: /Discover, /Ask, /Upload
binary/XML plist metadata, chunked HTTP, DVZIP/ODC CPIO payload
TLS over scoped IPv6 link-local sockets
mDNS _airdrop._tcp service discovery
IPv6 awdl0-style interface
AWDL action/data frames and availability windows
802.11 monitor/injection or native firmware AWDL
```

Modern compatibility behavior is represented by one `AirDropCapabilities` value rather than distributed iOS-version conditionals. The current model records chunked upload, transfer-ID, DVZIP, and multiple-file requirements.

## Discovery and transfer order

Receiving opens a bounded radio/application lease, publishes the AirDrop mDNS service, handles `/Discover`, parses `/Ask` metadata under configured limits, and presents an approval request before `/Upload` data becomes user-visible. Upload bytes remain in a private staging directory until archive validation and a collision-free final move succeed.

Sending starts a bounded BLE wake advertisement only after explicit user intent, discovers AWDL peers, resolves AirDrop service/name data, sends `/Discover`, sends `/Ask` with a fresh transfer identifier and complete file metadata, waits for remote approval, then streams one chunked DVZIP container to `/Upload`. Cancellation stops the transport and releases BLE/radio session references.

## Implemented in this repository

- Compatibility flags and stable transfer state types.
- Size/count/metadata limit checks before upload.
- Single-component filename validation and atomic collision-free destination reservation.
- Explicit `http`/`https` URL allowlist; received URLs are never opened automatically.
- AWDL radio trait, deterministic simulated backend, TSFT-to-monotonic bridge, timestamp-wrap and availability-window tests.
- Verified Everyone-mode Continuity manufacturer payload serialization with a 30-second maximum lease.

## Still required for wire interoperability

The pinned GPL mDNS/TLS/plist/HTTP/DVZIP sender and real AWDL frame state machine are integrated. Their 370 protocol/radio unit tests run in addition to OmarchyDrop's boundary tests. `omdropd` exposes real discovery and multi-file send only while `awdl0` is healthy. Physical adapter/iPhone validation, streaming send progress, cancellation, and BLE wake still remain.

The incoming endpoint is deliberately not exposed yet. The upstream receiver auto-accepts `/Ask`, buffers uploads, and does not enforce OmarchyDrop's destination/size policy. It will remain disabled until `/Ask` is bound to a local approval token and `/Upload` is streamed with byte, archive-member, decompression, path, collision, timeout, and disk-space limits.

## BLE wake provenance

The Everyone-mode payload uses Apple company ID `0x004c` and manufacturer bytes beginning `05 12`, matching the bounded BlueZ D-Bus advertisement measured by `airdrop-mt7921`. Empty contact hashes target Everyone mode only. Contacts Only is outside version 0.1 and no Apple credentials are requested.

