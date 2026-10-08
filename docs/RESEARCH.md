# OmarchyDrop research notes

Research snapshot: 2026-10-07. The repository revisions below are pinned observations, not claims that every later release has identical behavior.

## Omarchy and Quickshell

The cloud image is not an Omarchy desktop: `OMARCHY_PATH` is unset and there is no running Quickshell session. The current upstream `basecamp/omarchy` tree was inspected at commit `0f8af9be307d5d4f12cc0f6394892cac651ed5e6` (2026-10-06).

The current shell is one long-running Quickshell process. A third-party plugin is a Git repository containing a schema-version-1 `manifest.json`; the `omarchy.*` namespace is reserved. Rich first-party connectivity widgets (`omarchy.network`, `omarchy.bluetooth`, and `omarchy.power`) are each a `bar-widget` whose entry point extends `qs.Ui.Panel`. The panel host supplies popup coordination, outside-click handling, Escape handling, theming, and per-monitor lifecycle. Entry points are `Item`s, never `ShellRoot`s.

The current manifest contract requires `schemaVersion`, `id`, `name`, `version`, `kinds`, and matching `entryPoints`. A bar widget may also declare `displayName`, `category`, `allowMultiple`, `defaultSection`, defaults, and a settings schema. Third-party visual plugins receive a capability-scoped bar facade but still execute unsandboxed in the user's shell process. Consequently, this plugin keeps protocol parsing, radio control, filesystem access, and transfers in a separate daemon.

Quickshell's `Process` plus `SplitParser` supports a long-running JSON-lines event stream. Actions may be short-lived CLI invocations, but status must not be implemented as repeated process spawning. Omarchy's built-in polkit service is the desktop authentication agent; privileged setup must use system polkit rather than implementing a password UI.

## Reference implementations

### SEEMOO OWL

Inspected `seemoo-lab/owl` at `da255a70f221784c836d943dd3f243bc798f223b` (GPLv3).

OWL demonstrates a userspace AWDL implementation built around nl80211, monitor-mode frame capture/injection, AWDL election and synchronization, and a TAP-backed `awdl0` interface. Its documentation explicitly requires working frame injection and warns that monitor capability alone is insufficient. It recommends tested Atheros hardware, uses radiotap/TSFT information, does not preserve an infrastructure connection, and describes its channel sequence as static. Those limits are carried into OmarchyDrop's capability model instead of being hidden.

### SEEMOO OpenDrop

Inspected `seemoo-lab/opendrop` at `11fe7ba7861093b302bc0637e8cb10adf2d29337` (GPLv3).

OpenDrop separates AirDrop's mDNS, TLS, plist, HTTP, and archive behavior from AWDL. It documents `_airdrop._tcp`, `/Discover`, `/Ask`, and `/Upload`, and relies on an AWDL-capable interface on Linux. It is valuable protocol evidence but is not copied silently: derivative work must remain GPL-compatible and attributed.

### opendrop-rs

Inspected `ayourtch-llm/opendrop-rs` at `dccc798e244363eb92d35e3c52e9a913188dda91` (GPL-3.0-only).

This project explicitly identifies itself as a Rust derivative of OWL and OpenDrop. It splits the stack into `filin` (AWDL) and `luftlift` (AirDrop), uses a TAP interface, radiotap TSFT-to-monotonic-time bridging, rustls, mDNS, chunked HTTP, DVZIP framing, and ODC CPIO. Its documented hardware result is Apple-to-Linux receive on a carl9170 adapter; its README also says waking an idle iPhone for sending is not implemented. OmarchyDrop treats that as a reusable/pinnable GPL backend and evidence, not proof of universal two-way support.

### airdrop-mt7921

Inspected `jedbillyb/airdrop-mt7921` at `9f22b77be06a325e66cf600910a494efa71d76d7` (GPLv3).

The project reports real send and receive on MT7921/MT7922 with iOS 26, normally in exclusive-radio mode. Its important MediaTek observation is operational: create and tune a plain monitor interface, then add a second active monitor interface for ACK behavior; disabling mt76 runtime power management/deep sleep is also necessary on the tested systems. It additionally supplies OpenDrop compatibility patches for chunked uploads, DVZIP, transfer IDs, multiple files, and a bounded BlueZ D-Bus wake advertisement. These are hardware-specific experimental findings, not a rule that every mt76 device is supported.

### Omdrop native Broadcom path

Inspected `brentkearney/omdrop-awdl` at `d0d406b7415c923525cda84a970284445ee53f0b` (GPL-2.0-only) and `brentkearney/omdrop-plugin` at `afd1af8381d36f05520e8a830f1018240c53b460` (MIT).

`omdrop-awdl` exposes firmware AWDL on selected Apple Silicon Broadcom devices through a patched `brcmfmac` and an `awdl0` interface. It is not a generic Broadcom solution and must remain an optional, separately distributed backend because it is Linux-kernel-derived GPL-2.0-only code. The plugin provides useful evidence for the current Omarchy panel contract and for successful two-way transfers on its listed Apple hardware, but does not solve Intel, MediaTek, generic mac80211, or ordinary Broadcom laptops.

## Linux radio and Bluetooth APIs

The current Linux `nl80211` netlink specification exposes interface types, supported interface combinations, monitor mode, channel operations, remain-on-channel operations, regulatory frequency attributes, and hardware timestamp configuration. These are necessary passive signals but cannot prove that an adapter reliably injects AWDL action and data frames. OmarchyDrop therefore distinguishes `Unknown`, `Candidate`, `Experimental`, and validated support, and reserves a disruptive active test for explicit user action.

The current BlueZ `org.bluez.LEAdvertisingManager1` D-Bus API exposes `RegisterAdvertisement`, `UnregisterAdvertisement`, active/supported instance counts, and controller capabilities. BLE wake belongs behind this API with a strict deadline and cleanup. Bluetooth being absent may limit discovery of sleeping receivers, but must not disable receive paths that do not require it.

## Conclusions for version 0.1

- The UI is a single native Omarchy `bar-widget`, not another shell process.
- The normal daemon is unprivileged; the radio boundary is a narrow separately packaged service.
- AirDrop and AWDL remain separate layers joined by an IPv6-capable interface.
- Hardware database entries are ranking hints. Passive probing never upgrades an adapter to `Supported`.
- A dedicated validated USB adapter is the safest general fallback because it leaves infrastructure Wi-Fi alone.
- Initial compatibility targets Everyone mode. No Apple password or automatic credential extraction is acceptable.
- Real two-way compatibility requires physical Apple devices and adapters and must be recorded in `docs/HARDWARE.md`; CI validates parsers, state, safety, and simulated radios only.

