"""Verify current release binaries with isolated data and synthetic loopback OAuth.

First run cargo build --release --workspace --all-features. Logs and run data
are retained in target/audits/social-sdk-activation. No browser is opened.
"""

import datetime
import plistlib
import hashlib
import json
import subprocess
import time
from pathlib import Path


def hashes(paths):
    return {str(path): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}


def main():
    repo = Path(__file__).resolve().parents[1]
    audit = repo / "target/audits/social-sdk-activation"
    audit.mkdir(parents=True, exist_ok=True)
    run = audit / f"release-{time.time_ns()}"
    run.mkdir()
    shipped = (repo / "runtime/data").resolve(strict=True)
    probe_source = repo / "scripts/facebook_oauth_probe.rs"
    inputs = hashes([Path(__file__).resolve(), probe_source,
                     repo / "scripts/facebook_oauth_cache_probe.rs",
                     repo / "scripts/facebook_oauth_application_probe.rs",
                     repo / "scripts/facebook_oauth_inline_probe.rs",
                     repo / "scripts/facebook_oauth_inline_probe/host.rs",
                     repo / "scripts/facebook_oauth_system_probe.rs",
                     repo / "scripts/facebook_oauth_system_probe/observer.rs",
                     repo / "scripts/facebook_oauth_batch_probe.rs", shipped / "scripts/game.lua"])
    (run / "inputs.json").write_text(json.dumps(inputs, indent=2))
    results = []

    def execute(name, command, markers=()):
        start = time.monotonic()
        log_path = run / f"{name}.log"
        with log_path.open("w") as log:
            process = subprocess.run(
                command, stdout=log, stderr=subprocess.STDOUT, timeout=240, cwd=repo
            )
        results.append({
            "name": name, "exit": process.returncode,
            "seconds": round(time.monotonic() - start, 2),
            "command": list(map(str, command)),
        })
        (run / "results.json").write_text(json.dumps(results, indent=2))
        print(results[-1], flush=True)
        if process.returncode:
            raise RuntimeError(f"{name} failed; see {log_path}")
        text = log_path.read_text()
        for marker in markers:
            if marker not in text:
                raise RuntimeError(f"{name} did not prove {marker!r}")

    probe = run / "original-oauth"
    library = repo / "target/release/libstella_script.rlib"
    headless = repo / "target/release/stella-headless"
    execute("compile", [
        "rustc", "--edition=2024", "-C", "opt-level=3", "-C", "lto=thin",
        probe_source, "--extern", f"stella_script={library}",
        "-L", f"dependency={repo / 'target/release/deps'}", "-o", probe,
    ])
    binaries = hashes([probe, headless, library])
    (run / "binaries.json").write_text(json.dumps(binaries, indent=2))
    for name in ["original-oauth", "headless-local"]:
        data_dir = run / ("oauth-data" if name == "original-oauth" else name)
        data_dir.mkdir()
        (data_dir / "appdata").mkdir()
        (data_dir / "data").symlink_to(shipped, target_is_directory=True)
        if name == "original-oauth":
            execute(name, [probe, data_dir / "data"], [
                "original_boot_frames=600",
                "browser_retry=opening; interim_completion=none",
                "url_before_resume=open; service_profile=published",
                "reopened_cache=current_token; old_user=retained",
                "sdk_token_cache=ready_without_browser; retained_profile=true",
                "implicit_resume=closed_login_failed; nil_error=true; late_callback=false",
                "original_frames=960; http=5; fallback_calls=0; compatibility_bindings=0",
            ])
        else:
            execute(name, [
                headless, "--data", data_dir / "data", "--local-services", "--frames", "600",
            ], ["boot completed", "advanced 600 frames"])
    cache = run / "isolated-facebook-preferences.plist"
    for name, counts in [("cache-seed", "frames=720; auth=1; graph=1"),
                         ("cache-restore", "frames=600; auth=0; graph=1"),
                         ("cache-empty", "frames=600; auth=0; graph=0")]:
        data_dir = run / name
        data_dir.mkdir()
        (data_dir / "appdata").mkdir()
        (data_dir / "data").symlink_to(shipped, target_is_directory=True)
        execute(name, [probe, data_dir / "data", name, cache], [
            f"{name}: {counts}; cache_error=none; fallback_calls=0; compatibility_bindings=0",
        ])
        cache_bytes = cache.read_bytes()
        if not cache_bytes.startswith(b"bplist00"):
            raise RuntimeError("SDK cache was not persisted as a binary property list")
        (run / f"{name}-preferences.plist").write_bytes(cache_bytes)
        if name == "cache-seed":
            # Age only the isolated synthetic SDK metadata to exercise the real
            # extension admission in a separate original-script process.
            extension_cache = run / "extension-facebook-preferences.plist"
            preferences = plistlib.loads(cache_bytes)
            token = preferences["FBAccessTokenInformationKey"]
            prefix = "com.facebook.sdk:TokenInformation"
            now = datetime.datetime.now(datetime.timezone.utc).replace(tzinfo=None)
            token[prefix + "RefreshDateKey"] = now - datetime.timedelta(days=2)
            token[prefix + "PermissionsRefreshDateKey"] = now - datetime.timedelta(days=2)
            token[prefix + "ExpirationDateKey"] = now + datetime.timedelta(days=2)
            extension_cache.write_bytes(plistlib.dumps(preferences, fmt=plistlib.FMT_BINARY))
            extension_data = run / "cache-extend"
            (extension_data / "appdata").mkdir(parents=True)
            (extension_data / "data").symlink_to(shipped, target_is_directory=True)
            execute("cache-extend", [probe, extension_data / "data", "cache-extend", extension_cache], [
                "cache-extend: SDK_state=514; profile_token=current; refresh_cache=persisted",
                "cache-extend: frames=600; auth=0; graph=1; cache_error=none; fallback_calls=0; compatibility_bindings=0",
            ])
            system_cache = run / "system-facebook-preferences.plist"
            preferences = plistlib.loads(cache_bytes)
            token = preferences["FBAccessTokenInformationKey"]
            token[prefix + "LoginTypeLoginKey"] = 1
            token[prefix + "RefreshDateKey"] = now
            token[prefix + "PermissionsRefreshDateKey"] = now
            token[prefix + "ExpirationDateKey"] = now + datetime.timedelta(days=2)
            system_cache.write_bytes(plistlib.dumps(preferences, fmt=plistlib.FMT_BINARY))
            system_data = run / "system-repair"
            (system_data / "appdata").mkdir(parents=True)
            (system_data / "data").symlink_to(shipped, target_is_directory=True)
            execute("system-repair", [probe, system_data / "data", "system-repair", system_cache], [
                "system-repair: SDK_state=514; original_profile=failed; automatic_replay=0; callbacks=application_thread",
                "system-repair: frames=720; auth=0; graph=2; renew=1; access=1; current_token=true; cache_error=none; fallback_calls=0; compatibility_bindings=0",
            ])
            repaired_bytes = system_cache.with_suffix(".repaired.plist").read_bytes()
            repaired = plistlib.loads(repaired_bytes)["FBAccessTokenInformationKey"]
            assert repaired[prefix + "TokenKey"] == "synthetic-system-release"
            assert repaired[prefix + "LoginTypeLoginKey"] == 0
            assert repaired[prefix + "ExpirationDateKey"] == datetime.datetime(4001, 1, 1)
            assert repaired[prefix + "PermissionsKey"] == token[prefix + "PermissionsKey"]
            assert repaired[prefix + "RefreshDateKey"] >= now
            assert plistlib.loads(system_cache.read_bytes()) == {}
            (run / "system-repair-preferences.plist").write_bytes(repaired_bytes)
    application_cache = run / "application-facebook-preferences.plist"
    application_data = run / "application-auth"
    (application_data / "appdata").mkdir(parents=True)
    (application_data / "data").symlink_to(shipped, target_is_directory=True)
    execute("application-auth", [probe, application_data / "data", "application-auth", application_cache], [
        "application-auth: frames=840; application=2; browser=1; graph=2; cache_error=none; fallback_calls=0; compatibility_bindings=0",
    ])
    for suffix, login_type, access_token in [
        ("app", 2, "synthetic-application-release"),
        ("browser", 3, "synthetic-browser-fallback-release"),
    ]:
        raw = application_cache.with_suffix(f".{suffix}.plist").read_bytes()
        assert raw.startswith(b"bplist00")
        token = plistlib.loads(raw)["FBAccessTokenInformationKey"]
        assert token[prefix + "TokenKey"] == access_token
        assert token[prefix + "LoginTypeLoginKey"] == login_type
        assert isinstance(token[prefix + "ExpirationDateKey"], datetime.datetime)
        assert len(token[prefix + "PermissionsKey"]) == 4
    assert plistlib.loads(application_cache.read_bytes()) == {}
    inline_cache = run / "inline-facebook-preferences.plist"
    inline_data = run / "inline-auth"
    (inline_data / "appdata").mkdir(parents=True)
    (inline_data / "data").symlink_to(shipped, target_is_directory=True)
    execute("inline-auth", [probe, inline_data / "data", "inline-auth", inline_cache], [
        "inline-auth: frames=1080; browser=4; dialog=4; graph=1; cache_error=none; fallback_calls=0; compatibility_bindings=0",
    ])
    inline_preferences = plistlib.loads(inline_cache.with_suffix(".inline.plist").read_bytes())
    token = inline_preferences["FBAccessTokenInformationKey"]
    assert token[prefix + "TokenKey"] == "synthetic-inline-release"
    assert token[prefix + "LoginTypeLoginKey"] == 4
    assert isinstance(token[prefix + "ExpirationDateKey"], datetime.datetime)
    assert len(token[prefix + "PermissionsKey"]) == 4
    assert plistlib.loads(inline_cache.read_bytes()) == {}
    now = datetime.datetime.now(datetime.timezone.utc).replace(tzinfo=None)
    token[prefix + "RefreshDateKey"] = now - datetime.timedelta(days=2)
    token[prefix + "PermissionsRefreshDateKey"] = now
    token[prefix + "ExpirationDateKey"] = now + datetime.timedelta(days=2)
    inline_restore_cache = run / "inline-restore-facebook-preferences.plist"
    inline_restore_cache.write_bytes(plistlib.dumps(inline_preferences, fmt=plistlib.FMT_BINARY))
    inline_restore_data = run / "inline-restore"
    (inline_restore_data / "appdata").mkdir(parents=True)
    (inline_restore_data / "data").symlink_to(shipped, target_is_directory=True)
    execute("inline-restore", [probe, inline_restore_data / "data", "inline-restore", inline_restore_cache], [
        "inline-restore: frames=600; browser=0; dialog=0; graph=1; cache_error=none; fallback_calls=0; compatibility_bindings=0",
    ])
    restored_inline = plistlib.loads(inline_restore_cache.with_suffix(".restored.plist").read_bytes())
    assert restored_inline == inline_preferences  # Login type4 is not SSO-extended.
    assert plistlib.loads(inline_restore_cache.read_bytes()) == {}
    for manifest in [inputs, binaries]:
        if hashes(map(Path, manifest)) != manifest:
            raise RuntimeError("verification inputs changed during the run")
    (audit / "release-latest.json").write_text(json.dumps({
        "root": str(run), "results": results,
    }, indent=2))
    print("verified release outputs", run, flush=True)


if __name__ == "__main__":
    main()
