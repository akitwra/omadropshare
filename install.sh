#!/bin/bash

set -euo pipefail

repo_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)

for command in git makepkg systemctl; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "OmarchyDrop installer: required command not found: $command" >&2
    echo "Install the Arch base-devel toolchain, then run this installer again." >&2
    exit 1
  fi
done

commit=$(git -C "$repo_root" rev-parse --verify "HEAD^{commit}")
if [[ ! $commit =~ ^[0-9a-f]{40}$ ]]; then
  echo "OmarchyDrop installer: could not resolve the installed plugin commit." >&2
  exit 1
fi

build_dir=$(mktemp -d /tmp/omarchy-drop-build.XXXXXX)
cleanup() {
  if [[ -n ${build_dir:-} && $build_dir == /tmp/omarchy-drop-build.* && -d $build_dir ]]; then
    rm -rf -- "$build_dir"
  fi
}
trap cleanup EXIT

install -Dm644 "$repo_root/packaging/PKGBUILD" "$build_dir/PKGBUILD"

echo "Building OmarchyDrop backend from commit $commit..."
(
  cd "$build_dir"
  OMDROP_COMMIT="$commit" makepkg -si --needed
)

systemctl --user daemon-reload
systemctl --user enable --now omdropd.service

for _attempt in {1..50}; do
  if omdropctl status >/dev/null 2>&1; then
    echo "OmarchyDrop backend is installed and running."
    omdropctl status
    echo
    echo "Next: inspect an adapter without changing it:"
    echo "  omdropctl adapters"
    echo "  omdropctl hardware test --adapter <interface>"
    echo "Do not start the radio on your active Wi-Fi interface until you are ready for that connection to pause."
    exit 0
  fi
  sleep 0.1
done

echo "OmarchyDrop installer: the user service did not become ready." >&2
systemctl --user --no-pager --full status omdropd.service >&2 || true
omdropctl status >&2 || true
exit 1
