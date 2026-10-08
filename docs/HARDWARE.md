# Hardware support

OmarchyDrop reports two different facts: what the kernel advertises and what has completed a real two-way AirDrop interoperability test. The first never substitutes for the second.

## Capability levels

| Level | Meaning |
| --- | --- |
| Unknown | The driver or required capabilities are not understood. |
| Unsupported | A known requirement is absent or a real test failed. |
| Candidate | Passive checks look plausible; injection and interoperability are unverified. |
| Supported experimental | A usable path exists, but coverage or recovery testing is incomplete. |
| Supported | Both directions, cleanup, and representative Apple devices were tested. |
| Preferred | Supported and recommended because it preserves infrastructure Wi-Fi or has stronger operational evidence. |

`omdropctl adapters` reports the current passive result. `omdropctl hardware test` intentionally refuses today because an incomplete active test would create false confidence. The future active test will require explicit confirmation, journal network state, inject both action and data probes, verify reception/acknowledgment, and restore the adapter on every exit path.

## Project validation matrix

No physical adapter has yet been validated by this repository. The table is deliberately empty rather than importing another project's result as our own.

| Adapter | Driver | Receive | Send | Concurrent Wi-Fi | Kernel | Apple peer | Evidence |
| --- | --- | --- | --- | --- | --- | --- | --- |
| _None yet_ | — | — | — | — | — | — | Hardware lab run required |

## Upstream evidence used as hints

- `carl9170` USB: reported as a known-good monitor injector by OWL/opendrop-rs. OmarchyDrop classifies an untested local device as `Candidate`, not `Supported`.
- MT7921/MT7922 (`mt7921e`/`mt7921u`): `airdrop-mt7921` reports real bidirectional transfers with an exclusive dual-monitor arrangement. Local firmware, kernel, timing, restoration, and suspend tests are still required.
- Intel (`iwlwifi`): capability probe only. Vendor identity does not establish injection or timestamp correctness.
- Realtek (`rtw*`): candidate at most; upstream measurements include devices that inject management frames but silently drop data frames.
- Apple-Silicon Broadcom: the separate `omdrop-awdl` kernel backend reports successful native firmware AWDL on specific chips. Ordinary Broadcom/brcmfmac laptops are not equivalent.

## USB fallback

A dedicated validated USB adapter is the preferred universal fallback:

```text
internal adapter -> normal infrastructure Wi-Fi
USB adapter      -> AWDL only
```

This avoids taking over the user's active connection and limits recovery scope. Hotplug selection and automatic ranking are planned after the first physical backend is integrated.

## Recording a validation

A support claim must record adapter IDs, driver, firmware, kernel, regulatory domain, monitor configuration, Apple model/iOS or macOS version, direction, multiple-file behavior, throughput, BLE wake result, Wi-Fi restoration, unplug/crash behavior, and suspend/resume behavior. A passive `monitor=yes` report is insufficient.

