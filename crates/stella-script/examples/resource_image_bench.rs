//! Repeat native resource constructors with private, identical file inputs.
//!
//! cargo run -p stella-script --release --example resource_image_bench -- sheet 20000
//! The scenarios `sheet`, `font`, and `assets` measure successful warm reloads.
//! They do not load game levels or use player data, network services, or a GPU.

use std::{
    error::Error,
    fs,
    path::PathBuf,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use stella_script::{StellaLua, TextFontBinding};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Result<Self, Box<dyn Error>> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path =
            std::env::temp_dir().join(format!("stella-image-bench-{}-{nonce}", std::process::id()));
        fs::create_dir(&path)?;
        let fixture = Self(path);
        fs::create_dir_all(fixture.data().join("nested"))?;
        fs::create_dir(fixture.0.join("appdata"))?;
        let image = image();
        let sheet = sheet();
        fs::write(fixture.data().join("nested/same.pvr"), &image)?;
        fs::write(fixture.data().join("nested/A.dat"), &sheet)?;
        fs::write(fixture.data().join("nested/F.dat"), font())?;
        fs::write(fixture.0.join("appdata/same.pvr"), &image)?;
        fs::write(fixture.0.join("appdata/A.dat"), &sheet)?;
        Ok(fixture)
    }

    fn data(&self) -> PathBuf {
        self.0.join("data")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn string(value: &str) -> Vec<u8> {
    let mut bytes = (value.len() as u16).to_be_bytes().to_vec();
    bytes.extend_from_slice(value.as_bytes());
    bytes
}

fn container(tag: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut bytes = b"KA3D".to_vec();
    bytes.extend_from_slice(&(payload.len() as u32 + 8).to_be_bytes());
    bytes.extend_from_slice(tag);
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

fn sheet() -> Vec<u8> {
    let mut payload = 1_u16.to_be_bytes().to_vec();
    payload.extend(string("same.pvr"));
    payload.extend(1_u16.to_be_bytes());
    payload.extend(string("S"));
    for value in [0_u16, 0, 2, 2, 1, 1] {
        payload.extend(value.to_be_bytes());
    }
    container(b"SPRT", &payload)
}

fn font() -> Vec<u8> {
    let mut payload = 1_u16.to_be_bytes().to_vec();
    payload.extend(string("same.pvr"));
    for value in [1_u16, 2, 1, u16::from(b'A'), 0, 0, 2, 7, 5] {
        payload.extend(value.to_be_bytes());
    }
    container(b"FONT", &payload)
}

fn image() -> Vec<u8> {
    let mut bytes = [
        52_u32,
        64,
        64,
        0,
        0x12,
        64 * 64 * 4,
        32,
        0xff,
        0xff00,
        0xff0000,
        0xff000000,
        u32::from_le_bytes(*b"PVR!"),
        1,
    ]
    .into_iter()
    .flat_map(u32::to_le_bytes)
    .collect::<Vec<_>>();
    bytes.extend_from_slice(&[71, 19, 103, 255].repeat(64 * 64));
    bytes
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let scenario = args.next().unwrap_or_else(|| "sheet".to_owned());
    let iterations = args.next().map_or(Ok(20000_u32), |value| value.parse())?;
    if iterations == 0 || args.next().is_some() {
        return Err(
            "use: resource_image_bench [sheet|font|assets] [positive iteration count]".into(),
        );
    }
    let call = match scenario.as_str() {
        "sheet" => "res.createSpriteSheet('nested/A.dat',true)",
        "font" => "res.createBitmapFont('nested/F.dat',true)",
        "assets" => "_G.Assets.createSpriteSheet('A','A.dat','same.pvr')",
        _ => return Err("unknown scenario; use sheet, font, or assets".into()),
    };
    let fixture = Fixture::new()?;
    let runtime = StellaLua::new_with_resolution(fixture.data(), 16, 16)?;
    runtime.execute_source(&format!("for _=1,100 do {call} end"))?;
    let started = Instant::now();
    runtime.execute_source(&format!("for _=1,{iterations} do {call} end"))?;
    let elapsed = started.elapsed();
    let pixels = if scenario == "font" {
        runtime.execute_source("res.useFont('F'); res.drawString('','A',0,0)")?;
        let commands = runtime.take_text_commands();
        let Some(TextFontBinding::Bitmap {
            decoded_image: Some(image),
            ..
        }) = commands[0].font_binding.as_ref()
        else {
            return Err("font constructor did not retain its decoded Image".into());
        };
        image.rgba.clone()
    } else {
        runtime.execute_source("res.drawSprite('S',0,0)")?;
        let commands = runtime.take_render_commands();
        let image = commands[0]
            .bound_region
            .as_ref()
            .and_then(|region| region.decoded_image.as_ref())
            .ok_or("sheet constructor did not retain its Image")?;
        image.rgba.clone()
    };
    if pixels != [71, 19, 103, 255].repeat(64 * 64) {
        return Err("constructor pixels do not match the literal input".into());
    }
    println!(
        "{}",
        serde_json::json!({
            "scenario": scenario,
            "iterations": iterations,
            "warmup_iterations": 100,
            "constructor_seconds": elapsed.as_secs_f64(),
            "constructor_microseconds": elapsed.as_secs_f64() * 1_000_000.0 / f64::from(iterations),
            "rgba_bytes": pixels.len(),
            "rgba_sum": pixels.iter().map(|value| u64::from(*value)).sum::<u64>(),
            "pixels_match_literal": true,
            "isolated_data_directory": fixture.0,
        })
    );
    Ok(())
}
