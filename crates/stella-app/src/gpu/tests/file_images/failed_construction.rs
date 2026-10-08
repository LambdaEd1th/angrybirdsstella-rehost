//! Failed native resource candidates preserve both queued and future pixels.

use super::*;

#[test]
fn failed_sheet_replacement_preserves_warm_gpu_pixels() {
    for failure in ["missing", "invalid-image", "invalid-geometry"] {
        let files = Files::new();
        files.sheet("A");
        files.image(OLD);
        let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
        runtime
            .execute_source(
                "res.createSpriteSheet('A.dat'); res.drawSprite('A',0,0); res.drawSprite('A',2,0)",
            )
            .unwrap();
        let mut assets = assets(&files);
        assert_halves(&pixels(&runtime, &mut assets), OLD, OLD);
        runtime.execute_source("res.drawSprite('A',0,0)").unwrap();
        match failure {
            "missing" => std::fs::remove_file(files.data().join("same.png")).unwrap(),
            "invalid-image" => std::fs::write(files.data().join("same.png"), b"corrupt").unwrap(),
            _ => {
                files.image(NEW);
                let mut payload = 1_u16.to_be_bytes().to_vec();
                string(&mut payload, "same.png");
                std::fs::write(files.data().join("A.dat"), envelope(b"SPRT", payload)).unwrap();
            }
        }
        assert!(
            runtime
                .execute_source("res.createSpriteSheet('A.dat',true)")
                .is_err(),
            "{failure}"
        );
        runtime.execute_source("res.drawSprite('A',2,0)").unwrap();
        assert_halves(&pixels(&runtime, &mut assets), OLD, OLD);
    }
}

#[test]
fn failed_font_replacement_preserves_selected_and_queued_gpu_glyphs() {
    for failure in ["missing", "invalid-image", "invalid-glyph-header"] {
        let files = Files::new();
        files.font();
        files.image(OLD);
        let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
        runtime.execute_source("res.createBitmapFont('F.dat'); res.useFont('F'); res.drawString('','A',0,0); res.drawString('','A',2,0)").unwrap();
        let mut assets = assets(&files);
        assert_halves(&pixels(&runtime, &mut assets), OLD, OLD);
        runtime
            .execute_source("res.drawString('','A',0,0)")
            .unwrap();
        match failure {
            "missing" => std::fs::remove_file(files.data().join("same.png")).unwrap(),
            "invalid-image" => std::fs::write(files.data().join("same.png"), b"corrupt").unwrap(),
            _ => {
                files.image(NEW);
                let mut payload = 1_u16.to_be_bytes().to_vec();
                string(&mut payload, "same.png");
                std::fs::write(files.data().join("F.dat"), envelope(b"FONT", payload)).unwrap();
            }
        }
        assert!(
            runtime
                .execute_source("res.createBitmapFont('F.dat',true)")
                .is_err(),
            "{failure}"
        );
        runtime
            .execute_source("res.drawString('','A',2,0)")
            .unwrap();
        assert_halves(&pixels(&runtime, &mut assets), OLD, OLD);
    }
}
