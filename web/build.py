#!/usr/bin/env python3
"""Build a complete static GitHub Pages artifact, including the original data."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent
EXPORTS = ["_main", "_malloc", "_free", "_stella_init", "_stella_frame", "_stella_flush", "_stella_packet", "_stella_error",
           "_stella_pointer", "_stella_key", "_stella_wheel", "_stella_active", "_stella_share_preview", "_stella_gamer_services_preview",
           "_stella_save", "_stella_cancel_account", "_stella_touches", "_stella_touch", "_stella_shutdown", "_stella_set_locale", "_stella_resize", "_stella_audio_packet",
           "_stella_account_frame", "_stella_account_packet", "_stella_account_pointer", "_stella_account_control", "_stella_account_key", "_stella_account_wheel", "_stella_account_focus", "_stella_account_edit", "_stella_account_editor",
           "_stella_rating_frame", "_stella_rating_packet", "_stella_rating_pointer", "_stella_rating_choose", "_stella_rating_focus", "_stella_rating_key", "_stella_platform_packet"]


def build(data: Path, output: Path) -> None:
    data, output = data.resolve(), output.resolve()
    if not (data / "scripts/game.lua").is_file():
        raise SystemExit(f"Missing extracted game data: {data}/scripts/game.lua")
    if shutil.which("emcc") is None:
        raise SystemExit("emcc is required. Install/activate Emscripten 6.0.1 and source emsdk_env.sh.")
    if output == ROOT or output in data.parents or data in output.parents or output == data:
        raise SystemExit("Output must be separate from the source and runtime data directories.")
    fonts = ROOT / "web/fonts"
    font = fonts / "NotoSansCJK-Regular.ttc"
    if hashlib.sha256(font.read_bytes()).hexdigest() != "b76b0433203017ca80401b2ee0dd69350349871c4b19d504c34dbdd80541690a":
        raise SystemExit("The bundled Noto Sans CJK fallback font does not match its pinned hash.")
    subprocess.run(["rustup", "target", "add", "wasm32-unknown-emscripten"], check=True)
    with tempfile.TemporaryDirectory(prefix="stella-pages-") as temporary:
        stage = Path(temporary) / "data"
        shutil.copytree(data, stage, ignore=shutil.ignore_patterns(".DS_Store", "._*", "__MACOSX"))
        flags = [
            "-sMODULARIZE=1", "-sEXPORT_ES6=1", "-sEXPORT_NAME=createStella",
            "-sENVIRONMENT=web,node", "-sEXIT_RUNTIME=0", "-sFORCE_FILESYSTEM=1",
            "-sALLOW_MEMORY_GROWTH=1", "-sINITIAL_MEMORY=268435456",
            "-sMAXIMUM_MEMORY=2147483648", "-sSTACK_SIZE=8388608",
            "-sEXPORTED_FUNCTIONS=" + json.dumps(EXPORTS, separators=(",", ":")),
            '-sEXPORTED_RUNTIME_METHODS=["FS","UTF8ToString","HEAPU8","HEAPF32"]',
            "--preload-file", str(stage) + "@/runtime/data",
            "--preload-file", str(fonts) + "@/runtime/host-fonts",
        ]
        env = os.environ.copy()
        # Apply application exports/preloading only to the final binary, not
        # dependency cdylibs. Structured argv also preserves paths with spaces.
        command = ["cargo", "rustc", "--locked", "--profile", "web", "-p", "stella-web",
                   "--bin", "stella-web", "--target", "wasm32-unknown-emscripten", "--"]
        for flag in flags:
            command.extend(["-C", "link-arg=" + flag])
        subprocess.run(command, cwd=ROOT, env=env, check=True)
        binary = ROOT / "target/wasm32-unknown-emscripten/web/deps"
        output.mkdir(parents=True, exist_ok=True)
        engine = output / "engine"
        engine.mkdir(exist_ok=True)
        # Keep Emscripten's generated sibling filenames so locateFile also
        # works under /<repository>/ on GitHub Pages.
        for suffix in ("js", "wasm", "data"):
            shutil.copyfile(binary / f"stella_web.{suffix}", engine / f"stella_web.{suffix}")
        digest = hashlib.sha256()
        for suffix in ("js", "wasm", "data"):
            with (engine / f"stella_web.{suffix}").open("rb") as resource:
                digest.update(hashlib.file_digest(resource, "sha256").digest())
        engine_version = digest.hexdigest()[:16]
        version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
        for name in ("index.html", "style.css", "theme.js", "launcher.js", "display.js", "storage.js", "backup.js", "renderer.js", "sharing.js", "gamer-services.js", "audio.js", "lifecycle.js", "input.js", "account.js", "rating.js", "platform-actions.js", "i18n.js", "locales.js", "app-icon.png"):
            if name in ("index.html", "launcher.js"):
                source = (ROOT / "web" / name).read_text()
                source = source.replace("__STELLA_ENGINE_VERSION__", engine_version)
                (output / name).write_text(source.replace("__STELLA_APP_VERSION__", version))
            else:
                shutil.copyfile(ROOT / "web" / name, output / name)
        shutil.copytree(ROOT / "web/assets", output / "assets", dirs_exist_ok=True)
        shutil.copytree(ROOT / "web/vendor", output / "vendor", dirs_exist_ok=True)
        (output / "fonts").mkdir(exist_ok=True)
        for name in ("OFL.txt", "README.md"):
            shutil.copyfile(fonts / name, output / "fonts" / name)
        (output / ".nojekyll").write_text("")
        (output / "package.json").write_text('{"type":"module"}\n')
        try:
            commit = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
        except (OSError, subprocess.CalledProcessError):
            commit = None
        (output / "build-info.json").write_text(json.dumps({
            "engine": "stella-web", "version": version, "commit": commit, "emscripten": "6.0.1"
        }) + "\n")
    print(f"Pages artifact ready: {output}\nPreview: python3 -m http.server 8000 --directory {output}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", type=Path, default=ROOT / "runtime/data")
    parser.add_argument("--output", type=Path, default=ROOT / "dist/pages")
    args = parser.parse_args()
    build(args.data, args.output)
