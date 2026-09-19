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
    /// Execute diagnostic Lua after boot and before deterministic frames.
    #[arg(long)]
    pre_eval: Vec<String>,
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
    /// Queue a Telepods QR payload and expose the virtual host scanner.
    #[arg(long)]
    telepod_code: Option<String>,
    /// Repeatable URL scheme exposed through Purple's canOpenURL boundary.
    #[arg(long = "installed-url-scheme", value_name = "SCHEME")]
    installed_url_schemes: Vec<String>,
    /// Enable persistent local account/cloud, achievement and social providers.
    #[arg(long)]
    local_services: bool,
    /// Base URL of a compatible Stella GameServer API v1 implementation.
    #[arg(long)]
    game_server_url: Option<String>,
    /// Full URL of a compatible identity/2.0/time endpoint.
    #[arg(long)]
    server_time_url: Option<String>,
    /// Full URL of a compatible apdrive/1 Assets manifest endpoint.
    #[arg(long)]
    assets_url: Option<String>,
    /// Base URL of a compatible identity/2.0 service.
    #[arg(long)]
    identity_url: Option<String>,
    /// Optional compatible-identity client id.
    #[arg(long, requires = "identity_url")]
    identity_client_id: Option<String>,
    /// Optional compatible-identity client signature.
    #[arg(long, requires = "identity_url")]
    identity_client_signature: Option<String>,
    /// Optional compatible-identity client salt.
    #[arg(long, requires = "identity_url")]
    identity_client_salt: Option<String>,
    /// File containing the exact replacement-provider signing key bytes.
    #[arg(long, requires = "identity_url", conflicts_with_all = ["identity_client_signature", "identity_client_salt"])]
    identity_client_key_file: Option<PathBuf>,
    /// Base URL of a compatible storage/1.0 service.
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

fn main() {
    let args = Args::parse();
    let identity_signing_key = match args
        .identity_client_key_file
        .as_ref()
        .map(std::fs::read)
        .transpose()
    {
        Ok(key) => key,
        Err(error) => {
            eprintln!("read explicit identity signing key file: {error}");
            std::process::exit(18);
        }
    };
    let runtime = match if args.list_missing {
        StellaLua::new_with_missing_global_diagnostics(args.data)
    } else {
        StellaLua::new(args.data)
    } {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("failed to create Lua runtime: {error}");
            std::process::exit(1);
        }
    };

    if args.local_services
        && let Err(error) = runtime.enable_local_services()
    {
        eprintln!("local services stopped: {error}");
        std::process::exit(9);
    }
    if !args.installed_url_schemes.is_empty()
        && let Err(error) =
            runtime.set_installed_url_schemes(args.installed_url_schemes.iter().map(String::as_str))
    {
        eprintln!("installed URL scheme configuration stopped: {error}");
        std::process::exit(18);
    }
    if let Some(url) = args.game_server_url.as_deref()
        && let Err(error) = runtime.set_game_server_url(url)
    {
        eprintln!("game server configuration stopped: {error}");
        std::process::exit(10);
    }
    if let Some(url) = args.server_time_url.as_deref()
        && let Err(error) = runtime.set_server_time_url(url)
    {
        eprintln!("server time configuration stopped: {error}");
        std::process::exit(11);
    }
    if let Some(url) = args.assets_url.as_deref()
        && let Err(error) = runtime.set_assets_url(url)
    {
        eprintln!("assets service configuration stopped: {error}");
        std::process::exit(12);
    }
    if let Some(url) = args.identity_url.as_deref()
        && let Err(error) = runtime.set_identity_url(url)
    {
        eprintln!("identity service configuration stopped: {error}");
        std::process::exit(15);
    }
    if (args.identity_client_id.is_some()
        || args.identity_client_signature.is_some()
        || args.identity_client_salt.is_some())
        && let Err(error) = runtime.set_identity_client(
            args.identity_client_id.as_deref(),
            args.identity_client_signature.as_deref(),
            args.identity_client_salt.as_deref(),
        )
    {
        eprintln!("identity client configuration stopped: {error}");
        std::process::exit(16);
    }
    if let Some(key) = identity_signing_key.as_deref()
        && let Err(error) = runtime.set_identity_signing_key(key)
    {
        eprintln!("identity signing configuration stopped: {error}");
        std::process::exit(18);
    }
    if let Some(url) = args.storage_url.as_deref()
        && let Err(error) = runtime.set_storage_url(url)
    {
        eprintln!("storage service configuration stopped: {error}");
        std::process::exit(13);
    }
    if (args.storage_access_token.is_some() || args.storage_signature.is_some())
        && let Err(error) = runtime.set_storage_credentials(
            args.storage_access_token.as_deref(),
            args.storage_signature.as_deref(),
        )
    {
        eprintln!("storage credentials configuration stopped: {error}");
        std::process::exit(14);
    }
    if let Some(url) = args.social_url.as_deref()
        && let Err(error) = runtime.set_social_url(url)
    {
        eprintln!("social service configuration stopped: {error}");
        std::process::exit(17);
    }

    if let Some(code) = args.telepod_code.as_deref()
        && let Err(error) = runtime
            .set_qr_scanner_available(true)
            .and_then(|_| runtime.submit_qr_code(code).map(|_| ()))
    {
        eprintln!("telepod code stopped: {error}");
        std::process::exit(8);
    }

    match runtime.boot(&args.script) {
        Ok(()) => {
            println!("boot completed");
            for source in &args.pre_eval {
                if let Err(error) = runtime.execute_source(source) {
                    eprintln!("pre-eval stopped: {error}");
                    std::process::exit(7);
                }
            }
            let mut audio_clock = AudioOutputClock::default();
            for frame in 0..args.frames {
                let audio_state = runtime.audio_output_state();
                let transitions =
                    audio_clock.synchronize(&audio_state, Duration::from_nanos(16_666_667));
                runtime.apply_audio_playback_transitions(&transitions);
                if let Err(error) = runtime.update(1.0 / 60.0).and_then(|_| runtime.draw()) {
                    eprintln!("frame {frame} stopped: {error}");
                    let missing = runtime.missing_globals();
                    if !missing.is_empty() {
                        eprintln!("missing globals: {}", missing.join(", "));
                    }
                    std::process::exit(5);
                }
                let audio_state = runtime.audio_output_state();
                let transitions = audio_clock.synchronize(&audio_state, Duration::ZERO);
                runtime.apply_audio_playback_transitions(&transitions);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_key_file_requires_endpoint_and_excludes_literal_signing() {
        let base = [
            "stella-headless",
            "--data",
            "synthetic-not-read",
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
