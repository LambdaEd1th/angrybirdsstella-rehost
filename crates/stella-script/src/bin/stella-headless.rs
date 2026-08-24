use std::{path::PathBuf, time::Duration};

use clap::Parser;
use stella_script::{AudioOutputClock, StellaLua};

#[derive(Debug, Parser)]
#[command(about = "Boot original Stella Lua bytecode without graphics")]
struct Args {
    #[arg(long)]
    data: PathBuf,
    #[arg(long, default_value = "scripts/game.lua")]
    script: String,
    /// Invoke these global Lua callbacks after loading the script.
    #[arg(long)]
    call: Vec<String>,
    /// Execute host-side Lua in the game's environment after boot.
    #[arg(long)]
    eval: Vec<String>,
    /// Print all globals created by the loaded script.
    #[arg(long)]
    list_globals: bool,
    /// Print missing data reads separately from actually invoked compatibility fallbacks.
    #[arg(long)]
    list_missing: bool,
    /// Advance a deterministic 60 Hz update/draw loop after boot.
    #[arg(long, default_value_t = 0)]
    frames: u32,
    /// Print render commands emitted by the last frame.
    #[arg(long)]
    dump_render: bool,
}

fn main() {
    let args = Args::parse();
    let runtime = match StellaLua::new(args.data) {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("failed to create Lua runtime: {error}");
            std::process::exit(1);
        }
    };

    match runtime.boot(&args.script) {
        Ok(()) => {
            println!("boot completed");
            let mut audio_clock = AudioOutputClock::default();
            for frame in 0..args.frames {
                if let Err(error) = runtime.update(1.0 / 60.0).and_then(|_| runtime.draw()) {
                    eprintln!("frame {frame} stopped: {error}");
                    let missing = runtime.missing_globals();
                    if !missing.is_empty() {
                        eprintln!("missing globals: {}", missing.join(", "));
                    }
                    std::process::exit(5);
                }
                let audio_state = runtime.audio_output_state();
                let finished =
                    audio_clock.synchronize(&audio_state, Duration::from_nanos(16_666_667));
                runtime.finish_audio_playbacks(&finished);
            }
            if args.frames > 0 {
                println!("advanced {} frames", args.frames);
            }
            if args.dump_render {
                for command in runtime.take_render_commands() {
                    println!("render {command:?}");
                }
            }
            for source in &args.eval {
                if let Err(error) = runtime.execute_source(source) {
                    eprintln!("eval stopped: {error}");
                    std::process::exit(6);
                }
            }
            for callback in &args.call {
                match runtime.call_global(callback) {
                    Ok(true) => println!("called {callback}"),
                    Ok(false) => println!("callback not defined: {callback}"),
                    Err(error) => {
                        eprintln!("callback {callback} stopped: {error}");
                        let missing = runtime.missing_globals();
                        if !missing.is_empty() {
                            eprintln!("missing globals: {}", missing.join(", "));
                        }
                        std::process::exit(3);
                    }
                }
            }
            if args.list_globals {
                match runtime.global_names() {
                    Ok(names) => println!("globals ({}):\n{}", names.len(), names.join("\n")),
                    Err(error) => {
                        eprintln!("failed to enumerate globals: {error}");
                        std::process::exit(4);
                    }
                }
            }
            if args.list_missing {
                let missing = runtime.missing_globals();
                let fallbacks = runtime.fallback_calls();
                let compatibility = runtime.compatibility_bindings();
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
        }
        Err(error) => {
            eprintln!("boot stopped: {error}");
            let missing = runtime.missing_globals();
            if !missing.is_empty() {
                eprintln!("missing globals: {}", missing.join(", "));
            }
            std::process::exit(2);
        }
    }
}
