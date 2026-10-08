"""Portable staging and external release-resource packaging with synthetic font bytes."""

import hashlib
import json
from pathlib import Path
import subprocess
import shutil
import sys
import tarfile
import tempfile
import unittest
import zipfile
from unittest.mock import patch

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

    def test_verified_archive_staging_includes_fonts_and_rejects_stale_inputs_first(self):
        fixture = self.root / "checkout"
        staging = fixture / ".github/scripts/stage-runtime-data.py"
        write(staging, (REPOSITORY / ".github/scripts/stage-runtime-data.py").read_bytes())
        write(fixture / ".github/scripts/runtime_native_assets.py", Path(assets.__file__).read_bytes())
        write(fixture / ".github/scripts/runtime-native-assets.json",
              json.dumps({"files": self.entries}).encode())
        self.stage()
        write(self.data / "scripts/game.lua", b"fixture game")
        archive = self.root / "runtime.tar.gz"

        def archive_runtime():
            with tarfile.open(archive, "w:gz") as bundle:
                bundle.add(self.data, arcname="data")
            return hashlib.sha256(archive.read_bytes()).hexdigest()

        def prepare(output, digest):
            return subprocess.run([sys.executable, str(staging), str(archive),
                                   "--sha256", digest, "--output", str(output)],
                                  capture_output=True, text=True)

        digest = archive_runtime()
        output = self.root / "release-data"
        result = prepare(output, digest)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        for entry in self.entries:
            self.assertEqual((output / entry["destination"]).read_bytes(),
                             (self.bundle / entry["source"]).read_bytes())
        self.assertEqual((output / "scripts/game.lua").read_bytes(), b"fixture game")
        before = {path.relative_to(output): path.read_bytes() for path in output.rglob("*") if path.is_file()}
        rejected = prepare(output, digest)
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("refusing to overwrite", rejected.stderr)
        self.assertEqual(before, {path.relative_to(output): path.read_bytes() for path in output.rglob("*") if path.is_file()})
        bad_output = self.root / "bad-output"
        rejected = prepare(bad_output, "0" * 64)
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("SHA-256 mismatch", rejected.stderr)
        self.assertFalse(bad_output.exists())
        (self.data / self.entries[-1]["destination"]).unlink()
        digest = archive_runtime()
        rejected = prepare(bad_output, digest)
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("required native runtime asset is missing", rejected.stderr)
        self.assertFalse(bad_output.exists())

    def test_release_archive_rejects_path_traversal_before_installing(self):
        fixture = self.root / "checkout/.github/scripts"
        staging = fixture / "stage-runtime-data.py"
        write(staging, (REPOSITORY / ".github/scripts/stage-runtime-data.py").read_bytes())
        write(fixture / "runtime_native_assets.py", Path(assets.__file__).read_bytes())
        write(fixture / "runtime-native-assets.json", json.dumps({"files": self.entries}).encode())
        archive = self.root / "unsafe.tar.gz"
        with tarfile.open(archive, "w:gz") as bundle:
            bundle.addfile(tarfile.TarInfo("data/../../escaped"))
        output = self.root / "release-data"
        result = subprocess.run([sys.executable, str(staging), str(archive),
                                 "--sha256", hashlib.sha256(archive.read_bytes()).hexdigest(),
                                 "--output", str(output)], capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unsafe runtime archive entry", result.stderr)
        self.assertFalse(output.exists())
        self.assertFalse((self.root / "escaped").exists())

    @unittest.skipUnless(all(shutil.which(tool) for tool in ("bash", "tar", "zip", "unzip")),
                         "external archive packaging requires shell tar/zip tools")
    def test_real_tar_zip_packages_include_external_fonts_and_reject_stale_data_first(self):
        fixture = self.root / "checkout"
        add_resources = fixture / ".github/scripts/add-runtime-data.sh"
        write(add_resources, (REPOSITORY / ".github/scripts/add-runtime-data.sh").read_bytes())
        write(fixture / ".github/scripts/runtime_native_assets.py", Path(assets.__file__).read_bytes())
        write(fixture / ".github/scripts/runtime-native-assets.json",
              json.dumps({"files": self.entries}).encode())
        self.stage()
        write(self.data / "scripts/game.lua", b"fixture game")
        dist = self.root / "dist"
        dist.mkdir()
        unix_root = self.root / "unix-package"
        write(unix_root / "stella-app", b"fixture executable")
        unix = dist / "unix.tar.gz"
        with tarfile.open(unix, "w:gz") as archive:
            archive.add(unix_root, arcname="stella-test")
        windows = dist / "windows.zip"
        with zipfile.ZipFile(windows, "w") as archive:
            archive.writestr("stella-app.exe", b"fixture executable")
        result = subprocess.run(["bash", str(add_resources), str(dist), str(self.data)],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        with tarfile.open(unix) as unix_package, zipfile.ZipFile(windows) as windows_package:
            for entry in self.entries:
                relative = "runtime/data/" + entry["destination"]
                expected = (self.bundle / entry["source"]).read_bytes()
                self.assertEqual(unix_package.extractfile("stella-test/" + relative).read(), expected)
                self.assertEqual(windows_package.read(relative), expected)
            self.assertEqual(unix_package.extractfile("stella-test/stella-app").read(), b"fixture executable")
            self.assertEqual(windows_package.read("stella-app.exe"), b"fixture executable")
            self.assertTrue(unix_package.getmember("stella-test/runtime/appdata").isdir())
            self.assertIn("runtime/appdata/", windows_package.namelist())
        before = {path: path.read_bytes() for path in (unix, windows)}
        (self.data / self.entries[-1]["destination"]).unlink()
        rejected = subprocess.run(["bash", str(add_resources), str(dist), str(self.data)],
                                  capture_output=True, text=True)
        self.assertNotEqual(rejected.returncode, 0)
        self.assertIn("required native runtime asset is missing", rejected.stderr)
        self.assertEqual(before, {path: path.read_bytes() for path in (unix, windows)})


if __name__ == "__main__":
    unittest.main()
