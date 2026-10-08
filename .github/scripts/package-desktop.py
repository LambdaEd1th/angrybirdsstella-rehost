#!/usr/bin/env python3
"""Copy one resource-embedded GUI executable per desktop release target."""

import argparse
import mmap
import os
from pathlib import Path
import re
import shutil
import struct
import sys

ROOT = Path(__file__).resolve().parents[2]
BUNDLE_MAGIC = b"STELLA-DESKTOP-RESOURCE-BUNDLE-V1\0"
TARGETS = {
    "aarch64-apple-darwin": ("macho", 0x100000C),
    "x86_64-pc-windows-msvc": ("pe", 0x8664),
    "aarch64-pc-windows-msvc": ("pe", 0xAA64),
    "x86_64-unknown-linux-gnu": ("elf", 62),
    "aarch64-unknown-linux-gnu": ("elf", 183),
}


def verify_executable(path, target):
    kind, architecture = TARGETS[target]
    with path.open("rb") as binary:
        if path.stat().st_size < 128:
            raise ValueError(f"executable is truncated: {path}")
        with mmap.mmap(binary.fileno(), 0, access=mmap.ACCESS_READ) as contents:
            if contents.find(BUNDLE_MAGIC) < 0:
                raise ValueError("executable has no embedded game resources (debug builds cannot be shipped)")
            if kind == "macho":
                if contents[:4] != b"\xcf\xfa\xed\xfe" or struct.unpack_from("<I", contents, 4)[0] != architecture:
                    raise ValueError("expected an ARM64 Mach-O executable")
            elif kind == "elf":
                if contents[:6] != b"\x7fELF\x02\x01" or struct.unpack_from("<H", contents, 18)[0] != architecture:
                    raise ValueError(f"ELF architecture does not match {target}")
            else:
                offset = struct.unpack_from("<I", contents, 0x3C)[0]
                if offset + 94 > len(contents) or contents[:2] != b"MZ" or contents[offset:offset + 4] != b"PE\0\0":
                    raise ValueError("expected a Windows PE executable")
                if struct.unpack_from("<H", contents, offset + 4)[0] != architecture:
                    raise ValueError(f"PE architecture does not match {target}")
                if struct.unpack_from("<H", contents, offset + 24 + 68)[0] != 2:
                    raise ValueError("Windows release must use the GUI subsystem, without a console")


def package(target, version, target_root, dist_root):
    if target not in TARGETS:
        raise ValueError(f"unsupported release target: {target}")
    if not re.fullmatch(r"v[0-9]+\.[0-9]+\.[0-9]+(?:[.-][0-9A-Za-z.-]+)?", version):
        raise ValueError(f"invalid release version: {version}")
    suffix = ".exe" if "windows" in target else ""
    source = target_root / target / "release" / f"stella-app{suffix}"
    verify_executable(source, target)
    destination = dist_root / f"angry-birds-stella-rehost-{version}-{target}{suffix}"
    dist_root.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    if suffix != ".exe":
        destination.chmod(0o755)
    verify_executable(destination, target)
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", choices=TARGETS)
    parser.add_argument("version")
    parser.add_argument("--verify", type=Path, help="validate an already packaged executable")
    args = parser.parse_args()
    try:
        if args.verify:
            verify_executable(args.verify, args.target)
        else:
            print(package(
                args.target, args.version,
                Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")),
                Path(os.environ.get("STELLA_DIST_DIR", ROOT / "dist")),
            ))
    except (OSError, ValueError, struct.error) as error:
        print(f"desktop package: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
