//! Desktop and deterministic screenshot command-line front end.

use super::*;

#[derive(Debug, Parser)]
#[command(about = "Cross-platform Angry Birds Stella Rust rehost")]
struct Args {
    #[arg(long, default_value = "runtime/data")]
    data: PathBuf,
    /// Initial native drawable width published to the original Lua scripts.
    #[arg(long, default_value_t = GAME_WIDTH)]
    width: u32,
    /// Initial native drawable height published to the original Lua scripts.
    #[arg(long, default_value_t = GAME_HEIGHT)]
    height: u32,
    /// Render a deterministic frame to PNG instead of opening a window.
    #[arg(long)]
    screenshot: Option<PathBuf>,
    #[arg(long, default_value_t = 60)]
    screenshot_frames: u32,
    /// Optional game-space click (x y) injected at `--click-frame`.
    #[arg(long, num_args = 2)]
    click: Option<Vec<f64>>,
    #[arg(long, default_value_t = 60)]
    click_frame: u32,
    /// Optional second deterministic click used for interaction sequences.
    #[arg(long, num_args = 2)]
    second_click: Option<Vec<f64>>,
    #[arg(long, default_value_t = 120)]
    second_click_frame: u32,
    /// Optional third deterministic click for full menu-to-level flows.
    #[arg(long, num_args = 2)]
    third_click: Option<Vec<f64>>,
    #[arg(long, default_value_t = 180)]
    third_click_frame: u32,
    /// Optional game-space drag (start_x start_y end_x end_y).
    #[arg(long, num_args = 4)]
    drag: Option<Vec<f64>>,
    #[arg(long, default_value_t = 300)]
    drag_frame: u32,
    #[arg(long, default_value_t = 30)]
    drag_duration: u32,
    /// Optional second deterministic drag for multi-level interaction flows.
    #[arg(long, num_args = 4)]
    second_drag: Option<Vec<f64>>,
    #[arg(long, default_value_t = 600)]
    second_drag_frame: u32,
    #[arg(long, default_value_t = 30)]
    second_drag_duration: u32,
    /// Optional third deterministic drag/hold for tutorial sequences.
    #[arg(long, num_args = 4)]
    third_drag: Option<Vec<f64>>,
    #[arg(long, default_value_t = 900)]
    third_drag_frame: u32,
    #[arg(long, default_value_t = 30)]
    third_drag_duration: u32,
    /// Optional fourth deterministic drag for multi-bird level flows.
    #[arg(long, num_args = 4)]
    fourth_drag: Option<Vec<f64>>,
    #[arg(long, default_value_t = 1200)]
    fourth_drag_frame: u32,
    #[arg(long, default_value_t = 30)]
    fourth_drag_duration: u32,
    /// Optional fifth deterministic drag/hold for a later bird ability.
    #[arg(long, num_args = 4)]
    fifth_drag: Option<Vec<f64>>,
    #[arg(long, default_value_t = 1500)]
    fifth_drag_frame: u32,
    #[arg(long, default_value_t = 30)]
    fifth_drag_duration: u32,
    /// Repeatable deterministic click encoded as `frame,x,y`.
    #[arg(long = "script-click", value_name = "FRAME,X,Y")]
    script_clicks: Vec<String>,
    /// Repeatable deterministic drag encoded as `frame,duration,x1,y1,x2,y2`.
    #[arg(long = "script-drag", value_name = "FRAME,DURATION,X1,Y1,X2,Y2")]
    script_drags: Vec<String>,
    /// Repeatable diagnostic Lua injection encoded as `frame,source`.
    #[arg(long = "script-eval", value_name = "FRAME,SOURCE")]
    script_evals: Vec<String>,
    /// Print missing data reads separately from actually invoked compatibility fallbacks.
    #[arg(long)]
    list_missing: bool,
    /// Execute diagnostic Lua after deterministic screenshot frames, or before
    /// opening the window when running interactively.
    #[arg(long)]
    eval: Option<String>,
    /// Queue a Telepods QR payload (for example hasbro.telepod.020) and expose
    /// the host's virtual scanner to the original Telepods menu.
    #[arg(long)]
    telepod_code: Option<String>,
    /// Repeatable URL scheme handled by an installed host application. This
    /// feeds Purple's recovered canOpenURL checks for cross-promotion data.
    #[arg(long = "installed-url-scheme", value_name = "SCHEME")]
    installed_url_schemes: Vec<String>,
    /// Keep retired account/cloud, achievement and social providers unavailable
    /// instead of using their persistent local replacements.
    #[arg(long)]
    offline_services: bool,
    /// Base URL of a compatible Stella GameServer API v1 implementation.
    /// Setting this enables the complete shipped online request facade.
    #[arg(long)]
    game_server_url: Option<String>,
    /// Full URL of a compatible identity/2.0/time endpoint returning a Unix
    /// timestamp as a JSON number or a `time`/`serverTime` field.
    #[arg(long)]
    server_time_url: Option<String>,
    /// Full URL of a compatible apdrive/1 Assets manifest endpoint. The
    /// endpoint receives one repeated `name` query parameter per asset.
    #[arg(long)]
    assets_url: Option<String>,
    /// Base URL of a compatible identity/2.0 service. The client appends the
    /// recovered `access`, `profile/own` and nickname-validation routes.
    #[arg(long)]
    identity_url: Option<String>,
    /// Optional client id accepted by the compatible identity service.
    #[arg(long, requires = "identity_url")]
    identity_client_id: Option<String>,
    /// Optional compatible-identity client signature.
    #[arg(long, requires = "identity_url")]
    identity_client_signature: Option<String>,
    /// Optional compatible-identity client salt.
    #[arg(long, requires = "identity_url")]
    identity_client_salt: Option<String>,
    /// File containing the exact replacement-provider signing key bytes.
    /// Generates a fresh native signature/salt pair for every access request.
    #[arg(long, requires = "identity_url", conflicts_with_all = ["identity_client_signature", "identity_client_salt"])]
    identity_client_key_file: Option<PathBuf>,
    /// Base URL of a compatible storage/1.0 service. The client appends the
    /// recovered `state` and `states/query` routes.
    #[arg(long)]
    storage_url: Option<String>,
    /// Optional compatible-storage X-Access-Token header value.
    #[arg(long, requires = "storage_url")]
    storage_access_token: Option<String>,
    /// Optional compatible-storage Rovio-Sgs header value.
    #[arg(long, requires = "storage_url")]
    storage_signature: Option<String>,
    /// Full URL of a compatible social JSON operation endpoint.
    #[arg(long)]
    social_url: Option<String>,
}

pub(super) fn run() -> Result<()> {
    let args = Args::parse();
    let identity_signing_key = args
        .identity_client_key_file
        .as_ref()
        .map(std::fs::read)
        .transpose()
        .map_err(|error| anyhow!("read explicit identity signing key file: {error}"))?;
    let resolution = GameResolution::new(args.width, args.height)?;
    let mut app = StellaApp::new_with_missing_global_diagnostics(
        args.data,
        resolution,
        args.list_missing,
        PlatformServiceOptions {
            telepod_code: args.telepod_code.as_deref(),
            installed_url_schemes: &args.installed_url_schemes,
            local_services: !args.offline_services,
            game_server_url: args.game_server_url.as_deref(),
            server_time_url: args.server_time_url.as_deref(),
            assets_url: args.assets_url.as_deref(),
            identity_url: args.identity_url.as_deref(),
            identity_client_id: args.identity_client_id.as_deref(),
            identity_client_signature: args.identity_client_signature.as_deref(),
            identity_client_salt: args.identity_client_salt.as_deref(),
            identity_signing_key: identity_signing_key.as_deref(),
            storage_url: args.storage_url.as_deref(),
            storage_access_token: args.storage_access_token.as_deref(),
            storage_signature: args.storage_signature.as_deref(),
            social_url: args.social_url.as_deref(),
        },
    )?;
    if let Some(destination) = args.screenshot {
        let mut clicks = Vec::new();
        if let Some(values) = args.click.as_deref() {
            clicks.push((args.click_frame, values[0], values[1]));
        }
        if let Some(values) = args.second_click.as_deref() {
            clicks.push((args.second_click_frame, values[0], values[1]));
        }
        if let Some(values) = args.third_click.as_deref() {
            clicks.push((args.third_click_frame, values[0], values[1]));
        }
        for encoded in &args.script_clicks {
            let fields = encoded.split(',').collect::<Vec<_>>();
            if fields.len() != 3 {
                return Err(anyhow!(
                    "invalid --script-click {encoded:?}; expected frame,x,y"
                ));
            }
            clicks.push((
                fields[0].parse().context("invalid script click frame")?,
                fields[1].parse().context("invalid script click x")?,
                fields[2].parse().context("invalid script click y")?,
            ));
        }
        let mut drags = Vec::new();
        if let Some(values) = args.drag.as_deref() {
            drags.push((
                args.drag_frame,
                args.drag_duration,
                values[0],
                values[1],
                values[2],
                values[3],
            ));
        }
        if let Some(values) = args.second_drag.as_deref() {
            drags.push((
                args.second_drag_frame,
                args.second_drag_duration,
                values[0],
                values[1],
                values[2],
                values[3],
            ));
        }
        if let Some(values) = args.third_drag.as_deref() {
            drags.push((
                args.third_drag_frame,
                args.third_drag_duration,
                values[0],
                values[1],
                values[2],
                values[3],
            ));
        }
        if let Some(values) = args.fourth_drag.as_deref() {
            drags.push((
                args.fourth_drag_frame,
                args.fourth_drag_duration,
                values[0],
                values[1],
                values[2],
                values[3],
            ));
        }
        if let Some(values) = args.fifth_drag.as_deref() {
            drags.push((
                args.fifth_drag_frame,
                args.fifth_drag_duration,
                values[0],
                values[1],
                values[2],
                values[3],
            ));
        }
        for encoded in &args.script_drags {
            let fields = encoded.split(',').collect::<Vec<_>>();
            if fields.len() != 6 {
                return Err(anyhow!(
                    "invalid --script-drag {encoded:?}; expected frame,duration,x1,y1,x2,y2"
                ));
            }
            drags.push((
                fields[0].parse().context("invalid script drag frame")?,
                fields[1].parse().context("invalid script drag duration")?,
                fields[2].parse().context("invalid script drag start x")?,
                fields[3].parse().context("invalid script drag start y")?,
                fields[4].parse().context("invalid script drag end x")?,
                fields[5].parse().context("invalid script drag end y")?,
            ));
        }
        let mut evals = Vec::new();
        for encoded in &args.script_evals {
            let Some((frame, source)) = encoded.split_once(',') else {
                return Err(anyhow!(
                    "invalid --script-eval {encoded:?}; expected frame,source"
                ));
            };
            evals.push((
                frame.parse().context("invalid script eval frame")?,
                source.to_owned(),
            ));
        }
        let result = (|| {
            app.save_screenshot(
                &destination,
                args.screenshot_frames,
                &clicks,
                &drags,
                &evals,
            )?;
            if args.list_missing {
                let missing = app.runtime.missing_globals();
                let fallbacks = app.runtime.fallback_calls();
                let compatibility = app.runtime.compatibility_bindings();
                println!(
                    "missing globals ({}):\n{}\ninvoked fallbacks ({}):\n{}\nremaining compatibility bindings ({}):\n{}",
                    missing.len(),
                    missing.join("\n"),
                    fallbacks.len(),
                    fallbacks.join("\n"),
                    compatibility.len(),
                    compatibility.join("\n")
                );
            }
            if let Some(source) = args.eval.as_deref() {
                app.runtime
                    .execute_source(source)
                    .map_err(|error| anyhow!(error.to_string()))?;
            }
            Ok(())
        })();
        app.finish_screenshot_run(result)?;
        println!("{}", destination.display());
        return Ok(());
    }
    if let Some(source) = args.eval.as_deref() {
        app.runtime
            .execute_source(source)
            .map_err(|error| anyhow!(error.to_string()))?;
    }
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_key_file_requires_endpoint_and_excludes_literal_signing() {
        let base = [
            "stella-app",
            "--identity-client-key-file",
            "synthetic-key-not-opened",
        ];
        assert!(Args::try_parse_from(base).is_err());
        let mut args = base.to_vec();
        args.extend(["--identity-url", "http://127.0.0.1:9/identity/3.0"]);
        let parsed = Args::try_parse_from(&args).unwrap();
        assert_eq!(
            parsed.identity_client_key_file.unwrap(),
            PathBuf::from("synthetic-key-not-opened")
        );
        for flag in ["--identity-client-signature", "--identity-client-salt"] {
            let mut conflicting = args.clone();
            conflicting.extend([flag, "literal"]);
            assert!(Args::try_parse_from(conflicting).is_err());
        }
    }
}
