#!/bin/bash

set -euo pipefail

radio_helper=/usr/lib/omarchy-drop/omdrop-radio
if [[ -x $radio_helper ]]; then
  while read -r unit _rest; do
    [[ $unit =~ ^omdrop-radio@([A-Za-z0-9_.-]{1,15})\.service$ ]] || continue
    adapter=${BASH_REMATCH[1]}
    if command -v pkexec >/dev/null 2>&1; then
      pkexec "$radio_helper" stop "$adapter"
    else
      sudo systemctl stop "$unit"
    fi
  done < <(systemctl list-units --all --plain --no-legend 'omdrop-radio@*.service')
fi

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
