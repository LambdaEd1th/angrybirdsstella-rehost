#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 <rust-target> <version>" >&2
  exit 2
fi

target="$1"
version="$2"
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
target_root="${CARGO_TARGET_DIR:-$root/target}"
binary_root="$target_root/$target/release"
package="angry-birds-stella-rehost-$version-$target"
package_root="$root/dist/$package"

rm -rf "$package_root"
mkdir -p "$package_root"

for binary in stella-app stella-headless stella-mp3-audit stella-tool; do
  install -m 0755 "$binary_root/$binary" "$package_root/$binary"
done

install -m 0644 "$root/README.md" "$package_root/README.md"
install -m 0644 "$root/LICENSE" "$package_root/LICENSE"

{
  echo "Version: $version"
  echo "Target: $target"
  echo "Commit: ${GITHUB_SHA:-$(git -C "$root" rev-parse HEAD)}"
  echo "Rust: $(rustc --version)"
} > "$package_root/BUILD-INFO.txt"

# macOS may attach provenance xattrs even to newly created package files.
# Suppress both PAX xattrs and AppleDouble records without changing GNU tar.
tar_create=(tar)
if [[ "$(uname -s)" == Darwin ]]; then
  tar_create+=(--no-xattrs)
fi
COPYFILE_DISABLE=1 "${tar_create[@]}" -C "$root/dist" -czf "$root/dist/$package.tar.gz" "$package"
rm -rf "$package_root"
echo "$root/dist/$package.tar.gz"
