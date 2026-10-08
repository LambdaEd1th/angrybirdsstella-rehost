#!/usr/bin/env python3
"""Verify the pinned runtime archive and prepare external resources for release packaging."""

import argparse
import hashlib
from pathlib import Path, PurePosixPath
import re
import shutil
import sys
import tarfile
import tempfile

from runtime_native_assets import verify


def stage(archive, expected_sha256, output):
    if not re.fullmatch(r"[0-9a-f]{64}", expected_sha256):
        raise ValueError("expected archive SHA-256 must contain 64 lowercase hex digits")
    digest = hashlib.sha256()
    with archive.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    actual = digest.hexdigest()
    if actual != expected_sha256:
        raise ValueError(f"runtime archive SHA-256 mismatch: expected {expected_sha256}, got {actual}")
    if output.exists() or output.is_symlink():
        raise ValueError(f"refusing to overwrite existing runtime data: {output}")
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".stella-release-data-", dir=output.parent) as temporary:
        data = Path(temporary) / "data"
        data.mkdir()
        with tarfile.open(archive, "r:gz") as bundle:
            for member in bundle:
                parts = PurePosixPath(member.name).parts
                if (not parts or parts[0] != "data" or ".." in parts
                        or any("\\" in part or ":" in part for part in parts)
                        or not (member.isfile() or member.isdir())):
                    raise ValueError(f"unsafe runtime archive entry: {member.name}")
                if any(part in (".DS_Store", "__MACOSX") or part.startswith("._") for part in parts):
                    raise ValueError(f"macOS metadata in runtime archive: {member.name}")
                if any("com.apple." in key for key in member.pax_headers):
                    raise ValueError(f"macOS extended attributes in runtime archive: {member.name}")
                destination = data.joinpath(*parts[1:])
                if member.isdir():
                    destination.mkdir(parents=True, exist_ok=True)
                else:
                    destination.parent.mkdir(parents=True, exist_ok=True)
                    with bundle.extractfile(member) as source, destination.open("xb") as target:
                        shutil.copyfileobj(source, target)
        if not (data / "scripts/game.lua").is_file():
            raise ValueError("runtime archive does not contain data/scripts/game.lua")
        verify(data)
        data.rename(output)
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--sha256", required=True)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    try:
        print(stage(args.archive, args.sha256, args.output))
    except (OSError, ValueError, tarfile.TarError) as error:
        print(f"release runtime data: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
