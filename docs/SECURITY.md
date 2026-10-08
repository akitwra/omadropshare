# Security model

OmarchyDrop assumes every nearby peer, wireless frame, application message, filename, archive entry, and URL is hostile. Everyone mode is unauthenticated by design.

## Trust boundaries

The Quickshell plugin is display/control code only. Its long-running child is `omdropctl events`; it does not parse protocol payloads or touch transferred files. `omdropd` is an unprivileged per-user daemon with a mode-0600 Unix socket. The radio service is a separate privilege boundary with only adapter-scoped session operations and `CAP_NET_ADMIN`/`CAP_NET_RAW` where unavoidable.

Polkit requires administrator authentication before the fixed radio helper is
executed. The helper validates adapter identity, accepts no caller-supplied
channel or path, and never evaluates shell commands or unrestricted raw frames.
No passwordless privilege-escalation policy or setuid main daemon is
acceptable.

## Nearby attacker

Threats include malformed radiotap/802.11/AWDL frames, election manipulation, malicious BLE advertisements, oversized plist values, chunk smuggling, decompression bombs, malformed CPIO, repeated approval prompts, transfer-ID confusion, connection exhaustion, and disk exhaustion. Parsers use bounded lengths and treat malformed input as an ordinary error. Progress and peer events are throttled; peers expire; approval attempts will be rate-limited.

Default incoming limits are 5 GiB compressed, 10 GiB decompressed, 1,000 files, 4 MiB metadata, and 255 encoded bytes per filename. Free-space reservation must leave a safety margin before accepting. These are upper bounds, not promises that all transfers are buffered in memory.

## Filesystem safety

Incoming data is staged under a private runtime directory. Absolute paths, `..`, nested paths in the initial file-only version, special files, and escaping links are rejected. Final destinations use atomic create-new semantics and suffix collisions (`photo-1.jpg`); existing files are never truncated. Decline, cancel, timeout, and failure remove staging content.

URLs are displayed as inert text. Only parsed `http` and `https` URLs may be offered through explicit Copy/Open actions. `file`, `javascript`, `data`, `ssh`, and custom schemes are rejected.

## Local attacker

The IPC protocol is versioned, line-delimited JSON with a 64 KiB request ceiling. The socket and containing runtime directory are restricted to the user. Stable error codes cross the UI boundary; raw OS/Rust details stay in local logs. Logs never include file contents, private keys, validation records, tokens, full packet bodies, or unredacted long-lived peer identifiers.

## Recovery and availability

Radio sessions keep a root-owned journal of interface type, link state, and
NetworkManager ownership. The system unit runs restoration through
`ExecStopPost` after normal stop, startup failure, or process exit. Explicit
stale-session recovery through `omdropctl recover` is not wired yet and must
not claim that it restored anything. The project never globally stops
NetworkManager.

## Developer capture

Packet capture is not implemented. Any future capture command must be explicit, duration-bounded, off by default, stored privately, and warn that nearby-device metadata may be present. Captures are never telemetry; OmarchyDrop sends no analytics or transfer metadata to project servers.

## Privileged radio boundary

`omdropd` and the QML plugin remain unprivileged. `/usr/lib/omarchy-drop/omdrop-radio` is a root-owned, fixed-command helper invoked through polkit. It accepts only validated Linux interface names and the closed verbs `start`, `stop`, `run`, and `restore`; it never evaluates a shell command or caller-supplied path. The system unit bounds the radio process to `CAP_NET_ADMIN` and `CAP_NET_RAW` and `/dev/net/tun`.

Before monitor mode, the helper records the adapter type, administrative link state, and NetworkManager ownership in a root-only runtime journal. `ExecStopPost` restores that state after normal stop, startup failure, or crash. No persistent NetworkManager setting or general privilege-escalation rule is installed.

