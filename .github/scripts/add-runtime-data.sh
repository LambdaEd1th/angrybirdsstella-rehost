#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <dist-directory> <verified-runtime-data-directory>" >&2
  exit 2
fi

dist_dir="$(cd "$1" && pwd)"
data_dir="$(cd "$2" && pwd)"
repository_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

# This is packaging only. The game reads external files and never extracts them.
# Validate native fonts before changing any platform archive.
if [[ ! -f "$data_dir/scripts/game.lua" ]]; then
  echo "runtime data does not contain scripts/game.lua" >&2
  exit 1
fi
python3 "$repository_root/.github/scripts/runtime_native_assets.py" verify --data "$data_dir"

work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT
tar_archives=()
zip_archives=()
for archive in "$dist_dir"/*.tar.gz; do
  [[ -f "$archive" ]] && tar_archives+=("$archive")
done
for archive in "$dist_dir"/*.zip; do
  [[ -f "$archive" ]] && zip_archives+=("$archive")
done
if (( ${#tar_archives[@]} + ${#zip_archives[@]} == 0 )); then
  echo "no platform packages found in $dist_dir" >&2
  exit 1
fi

tar_create=(tar)
if [[ "$(uname -s)" == Darwin ]]; then
  tar_create+=(--no-xattrs)
fi

for archive in "${tar_archives[@]}"; do
  package_work="$work_dir/tar-package"
  rm -rf "$package_work"
  mkdir -p "$package_work"
  tar -xzf "$archive" -C "$package_work"
  package_roots=("$package_work"/*)
  if (( ${#package_roots[@]} != 1 )) || [[ ! -d "${package_roots[0]}" ]]; then
    echo "expected one package root in $(basename "$archive")" >&2
    exit 1
  fi
  package_root="${package_roots[0]}"
  mkdir -p "$package_root/runtime/appdata"
  cp -R "$data_dir" "$package_root/runtime/data"
  package_name="$(basename "$package_root")"
  COPYFILE_DISABLE=1 "${tar_create[@]}" -C "$package_work" -czf "$archive" "$package_name"
done

for archive in "${zip_archives[@]}"; do
  package_work="$work_dir/zip-package"
  rm -rf "$package_work"
  mkdir -p "$package_work"
  unzip -q "$archive" -d "$package_work"
  mkdir -p "$package_work/runtime/appdata"
  cp -R "$data_dir" "$package_work/runtime/data"
  rm -f "$archive"
  (
    cd "$package_work"
    COPYFILE_DISABLE=1 zip -qr "$archive" .
  )
done

echo "added external runtime/data to ${#tar_archives[@]} tar and ${#zip_archives[@]} zip packages"
