//! Desktop execution of platform actions accepted by the recovered Lua API.

use std::process::{Command, Stdio};

use super::*;

const APP_STORE_PRODUCT_PREFIX: &str = "https://apps.apple.com/app/id";

fn app_store_product_url(product_id: &str, _product_type: u32) -> String {
    format!("{APP_STORE_PRODUCT_PREFIX}{product_id}")
}

fn launch_external_target(target: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("/usr/bin/open");
        command.arg(target);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("rundll32.exe");
        command.args(["url.dll,FileProtocolHandler", target]);
        command
    };

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(target);
        command
    };

    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("open external target {target:?}"))?;
    Ok(())
}

impl StellaApp {
    pub(super) fn dispatch_platform_actions(&mut self) {
        for request in self.runtime.take_platform_action_requests() {
            let target = match request {
                PlatformActionRequest::OpenUrl { url } => url,
                PlatformActionRequest::OpenAppStoreProduct {
                    product_id,
                    product_type,
                } => app_store_product_url(&product_id, product_type),
            };
            if let Err(error) = launch_external_target(&target) {
                // UIApplication's boolean is advisory and a failed platform
                // launch does not stop Purple's update loop.
                eprintln!("platform action failed: {error}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_store_product_action_preserves_the_native_identifier() {
        assert_eq!(
            app_store_product_url("875251011", 3),
            "https://apps.apple.com/app/id875251011"
        );
    }
}
