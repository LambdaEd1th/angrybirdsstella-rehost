#!/usr/bin/env python3
"""Stage native bundle assets omitted by Purple.app/data extraction, or verify fonts.

Only the original Skynest asset subtree, its notification sound and the two
manifested font files are copied. UIKit nibs and the executable remain in the
source bundle. Copying file content (not metadata) avoids macOS xattrs.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys


MANIFEST = Path(__file__).with_name("runtime-native-assets.json")


def manifest_files():
    return json.loads(MANIFEST.read_text(encoding="utf-8"))["files"]


def checked_file(path, expected_hash):
    if not path.is_file() or path.is_symlink():
        raise ValueError(f"required native runtime asset is missing or a symlink: {path}")
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    if digest != expected_hash:
        raise ValueError(f"SHA-256 mismatch for {path}: expected {expected_hash}, got {digest}")


def verify(data):
    files = manifest_files()
    for entry in files:
        checked_file(data / entry["destination"], entry["sha256"])
    return len(files)


def is_metadata(name):
    return name in (".DS_Store", "__MACOSX") or name.startswith("._")


def stage(bundle, output):
    bundle = bundle.resolve()
    output = output.resolve()
    if bundle == output or bundle in output.parents or output in bundle.parents:
        raise ValueError("runtime output must be separate from the source application bundle")

    # Validate all source inputs before touching the destination. A font from
    # another release must not silently change native label metrics.
    copies = []
    entries = manifest_files()
    for entry in entries:
        source = bundle / entry["source"]
        checked_file(source, entry["sha256"])
        copies.append((source, output / entry["destination"]))
    skynest = bundle / "skynestdata"
    sound = bundle / "channel_push_notification.wav"
    if not skynest.is_dir() or skynest.is_symlink():
        raise ValueError(f"native Skynest resource directory is missing or a symlink: {skynest}")
    if not sound.is_file() or sound.is_symlink():
        raise ValueError(f"native channel notification sound is missing or a symlink: {sound}")
    for root, directories, names in os.walk(skynest, followlinks=False):
        directories[:] = sorted(name for name in directories if not is_metadata(name))
        for name in directories + sorted(names):
            source = Path(root) / name
            if is_metadata(name):
                continue
            if source.is_symlink():
                raise ValueError(f"native resource symlinks are not staged: {source}")
            if source.is_file():
                copies.append((source, output / source.relative_to(bundle)))
    copies.append((sound, output / sound.name))

    # Copy Skynest first and the explicitly mapped fonts last, so a future
    # source subtree cannot shadow the hash-validated root font files.
    copies = copies[len(entries):] + copies[:len(entries)]
    for source, destination in copies:
        if not destination.resolve().is_relative_to(output):
            raise ValueError(f"runtime destination escapes through a symlink: {destination}")
        destination.parent.mkdir(parents=True, exist_ok=True)
        if destination.is_symlink():
            raise ValueError(f"refusing to overwrite runtime symlink: {destination}")
        shutil.copyfile(source, destination)
    verify(output)
    return len(copies)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    staging = commands.add_parser("stage", help="append native assets to extracted or plain data")
    staging.add_argument("--bundle", required=True, type=Path, help="original Purple.app directory")
    staging.add_argument("--output", required=True, type=Path, help="runtime data root")
    verification = commands.add_parser("verify", help="check exact required font content")
    verification.add_argument("--data", required=True, type=Path, help="runtime data root")
    args = parser.parse_args()
    try:
        if args.command == "stage":
            count = stage(args.bundle, args.output)
            print(f"staged {count} native runtime files into {args.output}")
        else:
            count = verify(args.data)
            print(f"verified {count} native runtime font hashes in {args.data}")
    except (OSError, ValueError) as error:
        print(f"native runtime assets: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
