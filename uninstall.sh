#!/bin/bash

set -euo pipefail

if systemctl --user cat omdropd.service >/dev/null 2>&1; then
  systemctl --user disable --now omdropd.service
fi

omarchy-pkg-drop omarchy-drop-backend-git
systemctl --user daemon-reload

if command -v omdropctl >/dev/null 2>&1; then
  echo "OmarchyDrop uninstaller: omdropctl is still installed by another package." >&2
  exit 1
fi

echo "OmarchyDrop backend removed."
echo "Remove the QML plugin with: omarchy plugin remove io.github.akitwra.omarchy-drop"
