# Protocol engine

OmarchyDrop packages the `filin` AWDL link layer and `luftlift` AirDrop
application layer from
[`ayourtch-llm/opendrop-rs`](https://github.com/ayourtch-llm/opendrop-rs),
pinned to commit `dccc798e244363eb92d35e3c52e9a913188dda91`.

Both upstream and OmarchyDrop are GPL-3.0-only. The upstream license is
installed as `LICENSE.opendrop-rs` beside OmarchyDrop's license. The packaged
binaries live in `/usr/lib/omarchy-drop`, outside `PATH`, so only OmarchyDrop's
validated process boundary invokes them.

## Integration status

- `filin --check` is used for a non-disruptive monitor-mode preflight.
- The radio runtime requires `CAP_NET_ADMIN` and `CAP_NET_RAW`; only the
  root-owned system unit receives them. The long-lived user daemon receives
  neither. A fixed-argument helper journals and restores the adapter around
  every session, and polkit requires administrator authentication before it
  executes this root-owned helper.
- Standalone `luftlift receive` is deliberately not exposed. At the pinned
  revision it auto-accepts `/Ask`, buffers uploads in memory, and writes names
  without OmarchyDrop's collision/path/size policy. Real receiving remains
  disabled until the reviewed approval and streaming boundary is connected.
- Discovery and multi-file sending use the pinned `luftlift` library only after
  the radio service reports a healthy `awdl0` link. Peer selection is exact;
  there is no dangerous "first nearby device" fallback.

The exact commit and these constraints are tested in CI so an upstream branch
cannot silently replace the audited source.
