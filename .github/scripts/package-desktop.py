#!/usr/bin/env python3
"""Package one GUI executable with external runtime resources added separately."""

import argparse
import hashlib
import io
import json
import mmap
import os
from pathlib import Path
import re
import struct
import sys
import tarfile
import tempfile
import zipfile

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
            if contents.find(BUNDLE_MAGIC) >= 0:
                raise ValueError("embedded resources are no longer part of the desktop distribution")
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
    name = f"angry-birds-stella-rehost-{version}-{target}"
    dist_root.mkdir(parents=True, exist_ok=True)
    files = {
        f"stella-app{suffix}": source.read_bytes(),
        "README.md": (ROOT / "README.md").read_bytes(),
        "LICENSE": (ROOT / "LICENSE").read_bytes(),
        "BUILD-INFO.txt": (
            f"Version: {version}\nTarget: {target}\n"
            f"Commit: {os.environ.get('GITHUB_SHA', 'local')}\n"
        ).encode(),
    }
    if suffix == ".exe":
        destination = dist_root / f"{name}.zip"
        with zipfile.ZipFile(destination, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for path, contents in files.items():
                archive.writestr(path, contents)
    else:
        destination = dist_root / f"{name}.tar.gz"
        with tarfile.open(destination, "w:gz") as archive:
            for path, contents in files.items():
                entry = tarfile.TarInfo(f"{name}/{path}")
                entry.size = len(contents)
                entry.mode = 0o755 if path == "stella-app" else 0o644
                archive.addfile(entry, io.BytesIO(contents))
    return destination


def verify_package(path, target):
    """Verify the final archive's external layout, GUI program and native fonts."""
    suffix = ".exe" if "windows" in target else ""
    required = {f"stella-app{suffix}", "README.md", "LICENSE", "BUILD-INFO.txt"}
    with (zipfile.ZipFile(path) if suffix else tarfile.open(path, "r:gz")) as archive:
        prefix = "" if suffix else path.name.removesuffix(".tar.gz") + "/"
        entries = archive.infolist() if suffix else archive.getmembers()
        files = {}
        for entry in entries:
            name = entry.filename if suffix else entry.name
            if (name.startswith("/") or ".." in name.split("/") or "\\" in name
                    or any(part in (".DS_Store", "__MACOSX") or part.startswith("._")
                           for part in name.split("/"))):
                raise ValueError(f"unsafe or metadata package entry: {name}")
            if not suffix and (not (entry.isfile() or entry.isdir())
                               or any("com.apple." in key for key in entry.pax_headers)):
                raise ValueError(f"unsupported package entry: {name}")
            if not suffix and not name.startswith(prefix) and name != prefix.rstrip("/"):
                raise ValueError(f"unexpected package root: {name}")
            is_directory = entry.is_dir() if suffix else entry.isdir()
            if is_directory:
                continue
            relative = name[len(prefix):]
            if relative in files:
                raise ValueError(f"duplicate package entry: {name}")
            if relative not in required and not relative.startswith("runtime/data/"):
                raise ValueError(f"unexpected packaged file: {name}")
            files[relative] = entry
        for name in required | {"runtime/data/scripts/game.lua"}:
            if name not in files:
                raise ValueError(f"missing packaged file: {name}")

        def read(name):
            if suffix:
                return archive.read(files[name])
            with archive.extractfile(files[name]) as resource:
                return resource.read()

        # Reuse the executable validation without unpacking the distribution.
        with tempfile.TemporaryDirectory(prefix="stella-package-verify-") as temporary:
            executable = Path(temporary) / f"stella-app{suffix}"
            executable.write_bytes(read(f"stella-app{suffix}"))
            verify_executable(executable, target)
        manifest = json.loads((ROOT / ".github/scripts/runtime-native-assets.json").read_text())
        for resource in manifest["files"]:
            name = "runtime/data/" + resource["destination"]
            if name not in files:
                raise ValueError(f"required native runtime asset is missing: {name}")
            if hashlib.sha256(read(name)).hexdigest() != resource["sha256"]:
                raise ValueError(f"native runtime asset SHA-256 mismatch: {name}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", choices=TARGETS)
    parser.add_argument("version")
    parser.add_argument("--verify", type=Path, help="validate a final archive with external resources")
    args = parser.parse_args()
    try:
        if args.verify:
            verify_package(args.verify, args.target)
        else:
            print(package(
                args.target, args.version,
                Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target")),
                Path(os.environ.get("STELLA_DIST_DIR", ROOT / "dist")),
            ))
    except (OSError, ValueError, struct.error, tarfile.TarError, zipfile.BadZipFile) as error:
        print(f"desktop package: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
