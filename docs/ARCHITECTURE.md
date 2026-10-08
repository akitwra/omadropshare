# Architecture

OmarchyDrop is split by trust, privilege, and testability.

```text
Omarchy Quickshell (Panel.qml)
        | JSONL events + short CLI actions
        v
omdropd (unprivileged user daemon)
        | narrow versioned local IPC
        +--> AirDrop application layer (mDNS, TLS, HTTP, plist, containers)
        +--> BLE coordinator (bounded BlueZ advertisements)
        +--> hardware and session manager
                  | authenticated narrow radio API
                  v
            omdrop-radio-service
                  | nl80211 / AF_PACKET / TAP or native awdl0
                  v
              IPv6 awdl0
```

## Processes

`Panel.qml` renders state and emits user intent. It never probes hardware, parses packets, touches received archives, or performs network I/O. One long-running `omdropctl events --jsonl` child carries state changes. Action commands are bounded and return structured errors.

`omdropd` owns the application state machine, settings, peer expiry, transfer approvals, progress throttling, history, and reference-counted radio sessions. It runs as the desktop user and listens only on a mode-0600 Unix socket below `$XDG_RUNTIME_DIR/omarchy-drop`.

`omdrop-radio-service` is the eventual privileged boundary. Its public operations are adapter-scoped session operations, never arbitrary commands, paths, or raw frames supplied by an untrusted caller. The caller UID, selected adapter, session ownership, legal channel, and requested transition are validated. This component is not considered implemented merely because the unprivileged daemon exists.

## State model

The daemon exposes one top-level state: `Disabled`, `Starting`, `Ready`, `Discovering`, `Sending`, `Receiving`, `WaitingForApproval`, `HardwareUnsupported`, `BackendMissing`, `RadioUnavailable`, `BluetoothUnavailable`, or `Error`. Transfers separately move through `Preparing`, `WaitingForPeer`, `WaitingForLocalApproval`, `WaitingForRemoteApproval`, `Transferring`, `Finalizing`, and terminal states.

Discoverability is a lease, not a boolean. Expiry stops admitting new transfers but an active transfer keeps its radio-session reference until completion. BLE wake is also a lease and is stopped as soon as discovery/negotiation no longer needs it.

## Radio backends

The AWDL core consumes a `RadioBackend` interface with capability discovery, preparation, lifecycle, channel control, frame send, and frame receive. Planned concrete backends are:

- `NativeAwdlBackend`: an existing kernel/firmware `awdl0`, initially the separately packaged Apple-Silicon Broadcom implementation.
- `MediaTekMt76Backend`: the tested dual-monitor/exclusive technique, gated by runtime and active tests.
- `GenericMac80211Backend`: nl80211 plus AF_PACKET/radiotap, initially validated with dedicated carl9170 USB hardware.
- `SimulatedRadioBackend`: deterministic CI, parser, scheduler, and recovery tests.

Intel, Realtek, and generic Broadcom devices enter as `Candidate` unless an exact runtime active test and interoperability record proves more. Monitor mode alone is never a supported verdict.

## Recovery

Exclusive-mode acquisition is transactional. Before changes, the radio service journals interface, channel, NetworkManager ownership, and connection information under `/run/omarchy-drop`. Normal exit uses structured cancellation and RAII; signals request graceful stop; systemd `ExecStopPost` invokes bounded recovery; startup repairs stale journals. `omdropctl recover` exposes the same idempotent recovery path.

No backend may globally stop NetworkManager. A dedicated USB adapter is released/marked unmanaged individually. An integrated adapter requires explicit user consent before an exclusive session can disconnect Wi-Fi.

## Storage and input boundaries

Configuration lives under `$XDG_CONFIG_HOME/omarchy-drop`, history under `$XDG_STATE_HOME/omarchy-drop`, transient sockets/staging under `$XDG_RUNTIME_DIR/omarchy-drop`, and optional caches under `$XDG_CACHE_HOME/omarchy-drop`.

Incoming data is staged privately, size-accounted while streaming, and made visible only after acceptance and validation. Absolute paths, parent traversal, escaping links, special files, duplicate archive names, oversized metadata, excessive file counts, and decompression beyond configured limits are rejected. Final names use collision-free suffixes and never overwrite. Received URLs remain inert data; only `http` and `https` may be offered as explicit user actions.

## Current implementation boundary

The initial repository implements the UI/daemon contract, state machine, passive hardware classification, simulated AWDL timing/radio logic, safe transfer primitives, packaging boundaries, and hardware-independent tests. It deliberately reports real radio support as unavailable until a concrete backend and physical interoperability run have passed. This is a safe development base, not a false version-0.1 interoperability claim.

