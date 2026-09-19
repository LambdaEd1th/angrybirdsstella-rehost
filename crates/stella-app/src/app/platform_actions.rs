//! Desktop execution of platform actions accepted by the recovered Lua API.

use std::{
    fs,
    path::PathBuf,
    process::{Command, Stdio},
};

use super::*;

const APP_STORE_PRODUCT_PREFIX: &str = "https://apps.apple.com/app/id";

fn app_store_product_url(product_id: &str, _product_type: u32) -> String {
    format!("{APP_STORE_PRODUCT_PREFIX}{product_id}")
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn stage_gamer_services_view(
    view: GamerServicesView,
    entries: &[(String, String)],
) -> Result<PathBuf> {
    let (filename, heading, empty) = match view {
        GamerServicesView::Achievements => (
            "Angry Birds Stella Achievements.html",
            "Achievements",
            "No achievements unlocked yet.",
        ),
        GamerServicesView::Leaderboards => (
            "Angry Birds Stella Leaderboards.html",
            "Leaderboards",
            "No scores posted yet.",
        ),
    };
    let rows = if entries.is_empty() {
        format!("<p class=empty>{empty}</p>")
    } else {
        let items = entries
            .iter()
            .map(|(name, value)| {
                format!(
                    "<tr><td>{}</td><td>{}</td></tr>",
                    escape_html(name),
                    escape_html(value)
                )
            })
            .collect::<String>();
        format!("<table><tbody>{items}</tbody></table>")
    };
    let document = format!(
        "<!doctype html><meta charset=utf-8><title>Angry Birds Stella: Rehost — {heading}</title>\
         <style>body{{font:18px system-ui;background:#251746;color:#fff;max-width:760px;margin:48px auto;padding:0 24px}}\
         h1{{color:#ff8fe2}}table{{width:100%;border-collapse:collapse;background:#fff1;border-radius:14px;overflow:hidden}}\
         td{{padding:14px 18px;border-bottom:1px solid #fff2}}td:last-child{{text-align:right;color:#ffe36a}}\
         .empty{{padding:22px;background:#fff1;border-radius:14px}}</style><h1>{heading}</h1>{rows}"
    );
    let destination = std::env::temp_dir().join(filename);
    fs::write(&destination, document)
        .with_context(|| format!("stage local gamer-services view {}", destination.display()))?;
    Ok(destination)
}

pub(super) fn launch_external_target(target: &str) -> Result<()> {
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
        // Purple broadcasts each analytics event to the currently registered
        // providers and then releases it. The desktop application deliberately
        // registers no tracking endpoint, so consume the host boundary instead
        // of retaining an unbounded history during long sessions. Embedders
        // using StellaLua directly can drain and route these events themselves.
        drop(self.runtime.take_analytics_events());
        for request in self.runtime.take_platform_action_requests() {
            let target = match request {
                PlatformActionRequest::OpenUrl { url } => Ok(url),
                PlatformActionRequest::OpenAppStoreProduct {
                    product_id,
                    product_type,
                } => Ok(app_store_product_url(&product_id, product_type)),
                PlatformActionRequest::PlayVideo { path } => self
                    .runtime
                    .resolve_bundle_resource(&path)
                    .map(|path| path.to_string_lossy().into_owned())
                    .map_err(|error| anyhow!(error.to_string())),
                PlatformActionRequest::ShowGamerServices { view, entries } => {
                    stage_gamer_services_view(view, &entries)
                        .map(|path| path.to_string_lossy().into_owned())
                }
            };
            let result = target.and_then(|target| launch_external_target(&target));
            if let Err(error) = result {
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

    #[test]
    fn gamer_services_view_escapes_provider_text_and_preserves_values() {
        let path = stage_gamer_services_view(
            GamerServicesView::Leaderboards,
            &[("<LEVEL&1>".to_owned(), "902.5".to_owned())],
        )
        .unwrap();
        let html = fs::read_to_string(&path).unwrap();
        assert!(html.contains("&lt;LEVEL&amp;1&gt;"));
        assert!(html.contains("902.5"));
        fs::remove_file(path).unwrap();
    }
}
