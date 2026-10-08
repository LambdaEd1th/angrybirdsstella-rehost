"""External-resource desktop packaging regressions; no proprietary assets required."""

import importlib.util
from pathlib import Path
import struct
import tempfile
import tarfile
import zipfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("desktop_package", ROOT / ".github/scripts/package-desktop.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


def executable(target, embedded=False, subsystem=2):
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
        (binary_root / "runtime/data/game.lua").write_bytes(b"unverified build directory resource")
        return path

    def test_each_archive_contains_one_game_executable_without_developer_tools(self):
        for target in package.TARGETS:
            with self.subTest(target=target):
                source = self.source(target)
                dist = self.root / f"dist-{target}"
                output = package.package(target, "v1.2.3-beta4", self.target_root, dist)
                self.assertEqual(list(dist.iterdir()), [output])
                suffix = ".exe" if "windows" in target else ""
                expected = {f"stella-app{suffix}", "README.md", "LICENSE", "BUILD-INFO.txt"}
                if suffix:
                    self.assertEqual(output.suffix, ".zip")
                    with zipfile.ZipFile(output) as archive:
                        self.assertEqual(set(archive.namelist()), expected)
                        self.assertEqual(archive.read("stella-app.exe"), source.read_bytes())
                else:
                    prefix = output.name.removesuffix(".tar.gz") + "/"
                    with tarfile.open(output) as archive:
                        self.assertEqual(set(archive.getnames()), {prefix + name for name in expected})
                        self.assertEqual(archive.extractfile(prefix + "stella-app").read(), source.read_bytes())
                        self.assertEqual(archive.getmember(prefix + "stella-app").mode, 0o755)

    def test_previous_embedded_executable_is_rejected_before_output(self):
        target = "aarch64-apple-darwin"
        self.source(target, embedded=True)
        dist = self.root / "dist"
        with self.assertRaisesRegex(ValueError, "embedded resources"):
            package.package(target, "v1.2.3", self.target_root, dist)
        self.assertFalse(dist.exists())

    def test_final_archive_requires_external_resources(self):
        for target in package.TARGETS:
            with self.subTest(target=target):
                self.source(target)
                output = package.package(target, "v1.2.3", self.target_root, self.root / "dist")
                with self.assertRaisesRegex(ValueError, "runtime/data/scripts/game.lua"):
                    package.verify_package(output, target)

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
