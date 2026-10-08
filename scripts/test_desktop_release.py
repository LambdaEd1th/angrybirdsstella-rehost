"""Standalone desktop packaging regressions; no proprietary assets required."""

import importlib.util
from pathlib import Path
import struct
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("desktop_package", ROOT / ".github/scripts/package-desktop.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


def executable(target, embedded=True, subsystem=2):
    kind, architecture = package.TARGETS[target]
    contents = bytearray(256)
    if kind == "macho":
        contents[:4] = b"\xcf\xfa\xed\xfe"
        struct.pack_into("<I", contents, 4, architecture)
    elif kind == "elf":
        contents[:6] = b"\x7fELF\x02\x01"
        struct.pack_into("<H", contents, 18, architecture)
    else:
        contents[:2] = b"MZ"
        struct.pack_into("<I", contents, 0x3C, 128)
        contents[128:132] = b"PE\0\0"
        struct.pack_into("<H", contents, 132, architecture)
        struct.pack_into("<H", contents, 128 + 24 + 68, subsystem)
    if embedded:
        contents.extend(package.BUNDLE_MAGIC)
    return bytes(contents)


class DesktopReleaseTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="stella-desktop-package-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.target_root = self.root / "target"

    def source(self, target, **options):
        suffix = ".exe" if "windows" in target else ""
        binary_root = self.target_root / target / "release"
        binary_root.mkdir(parents=True, exist_ok=True)
        path = binary_root / f"stella-app{suffix}"
        path.write_bytes(executable(target, **options))
        for tool in ("stella-tool", "stella-headless", "stella-mp3-audit"):
            (binary_root / f"{tool}{suffix}").write_bytes(b"developer tool")
        (binary_root / "runtime/data").mkdir(parents=True)
        (binary_root / "runtime/data/game.lua").write_bytes(b"must not ship separately")
        return path

    def test_each_target_outputs_only_one_exact_game_executable(self):
        for target in package.TARGETS:
            with self.subTest(target=target):
                source = self.source(target)
                dist = self.root / f"dist-{target}"
                output = package.package(target, "v1.2.3-beta4", self.target_root, dist)
                self.assertEqual(list(dist.iterdir()), [output])
                self.assertEqual(output.read_bytes(), source.read_bytes())
                self.assertEqual(output.suffix == ".exe", "windows" in target)
                if "windows" not in target:
                    self.assertEqual(output.stat().st_mode & 0o777, 0o755)

    def test_unbundled_debug_executable_is_rejected_before_output(self):
        target = "aarch64-apple-darwin"
        self.source(target, embedded=False)
        dist = self.root / "dist"
        with self.assertRaisesRegex(ValueError, "debug builds cannot be shipped"):
            package.package(target, "v1.2.3", self.target_root, dist)
        self.assertFalse(dist.exists())

    def test_windows_console_executable_is_rejected(self):
        target = "x86_64-pc-windows-msvc"
        source = self.source(target, subsystem=3)
        with self.assertRaisesRegex(ValueError, "GUI subsystem"):
            package.verify_executable(source, target)

    def test_wrong_architecture_truncated_file_and_invalid_version_are_rejected(self):
        target = "x86_64-unknown-linux-gnu"
        source = self.source(target)
        with self.assertRaisesRegex(ValueError, "architecture"):
            package.verify_executable(source, "aarch64-unknown-linux-gnu")
        source.write_bytes(b"truncated")
        with self.assertRaisesRegex(ValueError, "truncated"):
            package.verify_executable(source, target)
        with self.assertRaisesRegex(ValueError, "invalid release version"):
            package.package(target, "../../unsafe", self.target_root, self.root / "dist")


if __name__ == "__main__":
    unittest.main()
