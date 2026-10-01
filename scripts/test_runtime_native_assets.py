"""Portable staging and real tar/zip embedding tests using synthetic font bytes."""

import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

import runtime_native_assets as assets


REPOSITORY = Path(__file__).resolve().parents[1]


def write(path, content):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(content)


class NativeRuntimeAssetsTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="stella-native-assets-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.bundle = self.root / "Purple.app"
        self.data = self.root / "runtime/data"
        self.entries = assets.manifest_files()
        for index, entry in enumerate(self.entries):
            content = f"synthetic font {index}".encode()
            entry["sha256"] = hashlib.sha256(content).hexdigest()
            write(self.bundle / entry["source"], content)
        write(self.bundle / "skynestdata/images/identity/button.png", b"identity artwork")
        write(self.bundle / "skynestdata/.DS_Store", b"metadata")
        write(self.bundle / "skynestdata/._button.png", b"metadata")
        write(self.bundle / "skynestdata/__MACOSX/nested", b"metadata")
        write(self.bundle / "channel_push_notification.wav", b"channel sound")
        write(self.bundle / "AccountView.nib", b"must remain in original bundle")

    def stage(self):
        with patch.object(assets, "manifest_files", return_value=self.entries):
            return assets.stage(self.bundle, self.data)

    def test_staging_preserves_lua_copies_fonts_and_skips_metadata_and_nibs(self):
        write(self.data / "scripts/game.lua", b"existing lossless Lua text")
        self.assertEqual(self.stage(), 4)
        for entry in self.entries:
            self.assertEqual((self.data / entry["destination"]).read_bytes(),
                             (self.bundle / entry["source"]).read_bytes())
        self.assertEqual((self.data / "scripts/game.lua").read_bytes(), b"existing lossless Lua text")
        self.assertTrue((self.data / "skynestdata/images/identity/button.png").is_file())
        self.assertTrue((self.data / "channel_push_notification.wav").is_file())
        self.assertFalse((self.data / "AccountView.nib").exists())
        self.assertFalse(any(assets.is_metadata(path.name) for path in self.data.rglob("*")))

    def test_bad_source_font_fails_before_writing_output(self):
        (self.bundle / self.entries[-1]["source"]).write_bytes(b"wrong version")
        with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
            self.stage()
        self.assertFalse(self.data.exists())

    def test_verification_rejects_missing_or_modified_fonts(self):
        self.stage()
        font = self.data / self.entries[0]["destination"]
        with patch.object(assets, "manifest_files", return_value=self.entries):
            self.assertEqual(assets.verify(self.data), 2)
            font.write_bytes(b"wrong bytes")
            with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
                assets.verify(self.data)
            font.unlink()
            with self.assertRaisesRegex(ValueError, "missing"):
                assets.verify(self.data)

    @unittest.skipUnless(all(shutil.which(tool) for tool in ("bash", "tar", "zip", "unzip")),
                         "release embedding requires shell tar/zip tools")
    def test_real_tar_zip_embedding_includes_fonts_and_rejects_stale_archive_first(self):
        # Run the production script in a fixture checkout with synthetic font
        # hashes. CI needs neither proprietary fonts nor a running application.
        fixture = self.root / "checkout"
        embed = fixture / ".github/scripts/embed-runtime-data.sh"
        write(embed, (REPOSITORY / ".github/scripts/embed-runtime-data.sh").read_bytes())
        write(fixture / ".github/scripts/runtime_native_assets.py", Path(assets.__file__).read_bytes())
        write(fixture / ".github/scripts/runtime-native-assets.json",
              json.dumps({"files": self.entries}).encode())
        self.stage()
        write(self.data / "scripts/game.lua", b"fixture game")
        archive = self.root / "runtime.tar.gz"

        def archive_runtime():
            with tarfile.open(archive, "w:gz") as package:
                package.add(self.data, arcname="data")

        archive_runtime()
        dist = self.root / "dist"
        dist.mkdir()
        unix_root = self.root / "unix-package"
        write(unix_root / "stella-app", b"fixture executable")
        unix = dist / "unix.tar.gz"
        with tarfile.open(unix, "w:gz") as package:
            package.add(unix_root, arcname="stella-test")
        windows = dist / "windows.zip"
        with zipfile.ZipFile(windows, "w") as package:
            package.writestr("stella-app.exe", b"fixture executable")
        result = subprocess.run(["bash", str(embed), str(dist), str(archive)],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        with tarfile.open(unix) as unix_package, zipfile.ZipFile(windows) as windows_package:
            for entry in self.entries:
                relative = "runtime/data/" + entry["destination"]
                expected = (self.bundle / entry["source"]).read_bytes()
                self.assertEqual(unix_package.extractfile("stella-test/" + relative).read(), expected)
                self.assertEqual(windows_package.read(relative), expected)

        before = {path: path.read_bytes() for path in (unix, windows)}
        (self.data / self.entries[-1]["destination"]).unlink()
        archive_runtime()
        rejected = subprocess.run(["bash", str(embed), str(dist), str(archive)],
                                  capture_output=True, text=True)
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("required native runtime asset is missing", rejected.stderr)
        self.assertEqual(before, {path: path.read_bytes() for path in (unix, windows)})


if __name__ == "__main__":
    unittest.main()
