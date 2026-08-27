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
    /// Execute diagnostic Lua after deterministic screenshot frames.
    #[arg(long)]
    eval: Option<String>,
    /// Queue a Telepods QR payload (for example hasbro.telepod.020) and expose
    /// the host's virtual scanner to the original Telepods menu.
    #[arg(long)]
    telepod_code: Option<String>,
}

pub(super) fn run() -> Result<()> {
    let args = Args::parse();
    let resolution = GameResolution::new(args.width, args.height)?;
    let mut app = StellaApp::new_with_missing_global_diagnostics(
        args.data,
        resolution,
        args.list_missing,
        args.telepod_code.as_deref(),
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
        println!("{}", destination.display());
        return Ok(());
    }
    let event_loop = EventLoop::new()?;
    event_loop.run_app(&mut app)?;
    Ok(())
}
