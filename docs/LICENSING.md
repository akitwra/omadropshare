# Licensing strategy

OmarchyDrop uses **Strategy A: an explicitly GPL-compatible derivative ecosystem**.

The project is licensed `GPL-3.0-only`. Its AWDL/AirDrop behavior is informed by and may link to or adapt GPLv3 work from SEEMOO OWL, SEEMOO OpenDrop, `opendrop-rs`, and `airdrop-mt7921`. Contributions that translate, adapt, or copy those works must preserve copyright notices, identify the source revision, and document the changed files. No contributor may relabel studied GPL implementation code as a clean-room implementation.

The current Omarchy plugin API and visual conventions were studied from `basecamp/omarchy`. Omarchy's source remains under its own license; this repository uses the public plugin contract and original QML rather than copying a first-party panel wholesale. The MIT-licensed `brentkearney/omdrop-plugin` is a behavioral and API reference and must be attributed if code is later incorporated.

`brentkearney/omdrop-awdl` is `GPL-2.0-only` because it derives from the Linux kernel. GPL-2.0-only is not generally compatible with combining code into a GPL-3.0-only program. That backend must therefore remain a separate kernel/module package and communicate through a process/kernel interface; its patches are not vendored or linked into this userspace workspace.

Dependencies retain their own notices. Packaging must ship the complete GPL text and source/provenance information required by each included work. Synthetic protocol fixtures created in this repository are preferred. Real packet captures may be committed only with documented redistribution permission and personal identifiers removed.

AirDrop is a trademark of Apple Inc. OmarchyDrop is independent software and is not affiliated with or endorsed by Apple.
