#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <dist-directory> <runtime-data.tar.gz>" >&2
  exit 2
fi

dist_dir="$(cd "$1" && pwd)"
data_archive="$(cd "$(dirname "$2")" && pwd)/$(basename "$2")"
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

mkdir -p "$work_dir/runtime"
tar -xzf "$data_archive" -C "$work_dir/runtime"

# Finder metadata is frequently introduced when the private runtime bundle is
# prepared on macOS.  The extracted tree is temporary and is the only source
# copied into release packages, so remove it here before validating or
# embedding the game data.  Keep this portable across GNU/BSD find (the
# release publish job runs on Linux while local archive preparation often runs
# on macOS).
find "$work_dir/runtime" -type f \( -name '.DS_Store' -o -name '._*' \) -print -delete
find "$work_dir/runtime" -type d -name '__MACOSX' -print -prune -exec rm -rf {} +

if [[ ! -f "$work_dir/runtime/data/scripts/game.lua" ]]; then
  echo "runtime data archive does not contain data/scripts/game.lua" >&2
  exit 1
fi

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
  cp -R "$work_dir/runtime/data" "$package_root/runtime/data"

  package_name="$(basename "$package_root")"
  tar -C "$package_work" -czf "$archive" "$package_name"
done

for archive in "${zip_archives[@]}"; do
  package_work="$work_dir/zip-package"
  rm -rf "$package_work"
  mkdir -p "$package_work/runtime/appdata"
  unzip -q "$archive" -d "$package_work"
  cp -R "$work_dir/runtime/data" "$package_work/runtime/data"

  rm -f "$archive"
  (
    cd "$package_work"
    zip -qr "$archive" .
  )
done

echo "embedded runtime data into ${#tar_archives[@]} tar and ${#zip_archives[@]} zip packages"
