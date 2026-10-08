//! Parameter-free shipping launcher using the existing external resource tree.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, ensure};

#[cfg(all(not(debug_assertions), not(test)))]
pub(super) fn run() -> Result<()> {
    let executable = std::env::current_exe().context("locate Stella executable")?;
    let working_directory = std::env::current_dir().context("read working directory")?;
    let data = resolve_data(&executable, &working_directory)?;
    let mut app = crate::StellaApp::new_with_missing_global_diagnostics(
        data,
        crate::GameResolution::default(),
        false,
        crate::PlatformServiceOptions {
            local_services: true,
            ..Default::default()
        },
    )?;
    let event_loop = crate::EventLoop::new().context("create event loop")?;
    let result = event_loop.run_app(&mut app).context("run Stella desktop");
    app.finish_window_run(result)
}

fn resolve_data(executable: &Path, working_directory: &Path) -> Result<PathBuf> {
    let adjacent_runtime = executable
        .parent()
        .context("Stella executable has no parent directory")?
        .join("runtime");
    // An installed runtime takes precedence even if incomplete: silently
    // switching to another installation would also switch its sibling saves.
    let runtime = if adjacent_runtime
        .try_exists()
        .with_context(|| format!("inspect {}", adjacent_runtime.display()))?
    {
        adjacent_runtime
    } else {
        // Preserve `cargo run --release` from the workspace root.
        working_directory.join("runtime")
    };
    let data = runtime.join("data");
    ensure!(
        data.join("scripts/game.lua")
            .try_exists()
            .with_context(|| format!("inspect external resources in {}", data.display()))?
            && data.join("scripts/game.lua").is_file(),
        "missing external game resources: {} (place runtime/data beside the executable)",
        data.display()
    );
    Ok(data)
}

#[cfg(all(target_os = "windows", not(debug_assertions), not(test)))]
pub(super) fn show_error(error: &anyhow::Error) {
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(
            window: *mut std::ffi::c_void,
            text: *const u16,
            caption: *const u16,
            flags: u32,
        ) -> i32;
    }
    let text: Vec<u16> = format!("Unable to start Angry Birds Stella: Rehost\n\n{error:#}")
        .encode_utf16()
        .chain([0])
        .collect();
    let caption: Vec<u16> = "Angry Birds Stella: Rehost"
        .encode_utf16()
        .chain([0])
        .collect();
    // Both UTF-16 buffers are NUL-terminated and live through the synchronous
    // Win32 dialog. No console exists in the shipping Windows subsystem.
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), caption.as_ptr(), 0x10);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        sync::atomic::{AtomicU64, Ordering},
    };

    static NEXT: AtomicU64 = AtomicU64::new(0);

    struct Sandbox(PathBuf);

    impl Sandbox {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "stella-external-launch-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(path)
        }

        fn resources(&self, location: &str) -> PathBuf {
            let data = self.0.join(location).join("runtime/data");
            fs::create_dir_all(data.join("scripts")).unwrap();
            fs::write(data.join("scripts/game.lua"), b"external resource fixture").unwrap();
            data
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }

    #[test]
    fn adjacent_resources_take_precedence_and_preserve_sibling_saves() {
        let sandbox = Sandbox::new();
        let adjacent = sandbox.resources("installed");
        let working = sandbox.resources("workspace");
        let saves = adjacent.parent().unwrap().join("appdata");
        fs::create_dir(&saves).unwrap();
        fs::write(saves.join("save.dat"), b"existing player save").unwrap();
        let resolved = resolve_data(
            &sandbox.0.join("installed/stella-app"),
            &sandbox.0.join("workspace"),
        )
        .unwrap();
        assert_eq!(resolved, adjacent);
        assert_eq!(resolved.parent().unwrap().join("appdata"), saves);
        assert_eq!(
            fs::read(saves.join("save.dat")).unwrap(),
            b"existing player save"
        );
        assert_eq!(
            fs::read(working.join("scripts/game.lua")).unwrap(),
            b"external resource fixture"
        );
    }

    #[test]
    fn workspace_resources_remain_available_for_cargo_run() {
        let sandbox = Sandbox::new();
        let data = sandbox.resources("workspace");
        assert_eq!(
            resolve_data(
                &sandbox.0.join("workspace/target/release/stella-app"),
                &sandbox.0.join("workspace"),
            )
            .unwrap(),
            data
        );
    }

    #[test]
    fn incomplete_adjacent_runtime_does_not_redirect_to_workspace_saves() {
        let sandbox = Sandbox::new();
        sandbox.resources("workspace");
        let adjacent = sandbox.0.join("installed/runtime");
        fs::create_dir_all(&adjacent).unwrap();
        let error = resolve_data(
            &sandbox.0.join("installed/stella-app"),
            &sandbox.0.join("workspace"),
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("missing external game resources")
        );
        assert!(error.to_string().contains(&adjacent.display().to_string()));
        assert_eq!(fs::read_dir(adjacent).unwrap().count(), 0);
    }

    #[test]
    fn missing_resources_fail_without_creating_or_extracting_files() {
        let sandbox = Sandbox::new();
        let error = resolve_data(&sandbox.0.join("stella-app"), &sandbox.0).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("missing external game resources")
        );
        assert_eq!(fs::read_dir(&sandbox.0).unwrap().count(), 0);
    }

    #[test]
    fn game_script_directory_is_not_accepted_as_a_resource_file() {
        let sandbox = Sandbox::new();
        let data = sandbox.0.join("runtime/data");
        fs::create_dir_all(data.join("scripts/game.lua")).unwrap();
        assert!(resolve_data(&sandbox.0.join("stella-app"), &sandbox.0).is_err());
        assert!(!sandbox.0.join("runtime/appdata").exists());
    }
}
