//! Actual framebuffer checks for native image input-stream routing.

use super::*;

#[test]
fn missing_sibling_reload_preserves_queued_and_warm_pixels_despite_decoys() {
    for kind in ["sheet", "font"] {
        let files = Files::new();
        files.sheet("A");
        files.font();
        files.image(OLD);
        std::fs::create_dir(files.data().join("nested")).unwrap();
        for filename in ["A.dat", "F.dat", "same.png"] {
            std::fs::rename(
                files.data().join(filename),
                files.data().join("nested").join(filename),
            )
            .unwrap();
        }
        let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
        let (create, left, right) = if kind == "sheet" {
            (
                "res.createSpriteSheet('nested/A.dat',true)",
                "res.drawSprite('A',0,0)",
                "res.drawSprite('A',2,0)",
            )
        } else {
            (
                "res.createBitmapFont('nested/F.dat',true)",
                "res.drawString('','A',0,0)",
                "res.drawString('','A',2,0)",
            )
        };
        runtime.execute_source(create).unwrap();
        if kind == "font" {
            runtime.execute_source("res.useFont('F')").unwrap();
        }
        runtime.execute_source(&format!("{left}; {right}")).unwrap();
        let mut catalog = assets(&files);
        assert_halves(&pixels(&runtime, &mut catalog), OLD, OLD);
        runtime.execute_source(left).unwrap();
        std::fs::remove_file(files.data().join("nested/same.png")).unwrap();
        files.image(NEW);
        assert!(runtime.execute_source(create).is_err(), "{kind}");
        runtime.execute_source(right).unwrap();
        assert_halves(&pixels(&runtime, &mut catalog), OLD, OLD);
    }
}

#[test]
fn leading_separator_image_uses_sibling_pixels_and_retains_them_after_deletion() {
    let files = Files::new();
    std::fs::create_dir(files.data().join("nested")).unwrap();
    files.image(OLD);
    std::fs::rename(
        files.data().join("same.png"),
        files.data().join("nested/same.png"),
    )
    .unwrap();
    files.image(NEW);
    let mut payload = 1_u16.to_be_bytes().to_vec();
    string(&mut payload, "/same.png");
    payload.extend_from_slice(&1_u16.to_be_bytes());
    string(&mut payload, "A");
    for value in [0_u16, 0, 2, 2, 0, 0] {
        payload.extend_from_slice(&value.to_be_bytes());
    }
    std::fs::write(
        files.data().join("nested/A.dat"),
        envelope(b"SPRT", payload),
    )
    .unwrap();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('nested/A.dat')")
        .unwrap();
    std::fs::remove_file(files.data().join("nested/same.png")).unwrap();
    std::fs::remove_file(files.data().join("nested/A.dat")).unwrap();
    runtime
        .execute_source("res.drawSprite('A',0,0); res.drawSprite('A',2,0)")
        .unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), OLD, OLD);
}

#[test]
fn assets_explicit_image_uploads_appdata_root_pixels_instead_of_sibling_decoy() {
    let files = Files::new();
    files.sheet("A");
    let appdata = files.0.join("appdata");
    std::fs::create_dir(appdata.join("nested")).unwrap();
    std::fs::rename(files.data().join("A.dat"), appdata.join("nested/A.dat")).unwrap();
    files.image(OLD);
    std::fs::rename(files.data().join("same.png"), appdata.join("same.png")).unwrap();
    files.image(NEW);
    std::fs::rename(
        files.data().join("same.png"),
        appdata.join("nested/same.png"),
    )
    .unwrap();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("_G.Assets.createSpriteSheet('A','nested/A.dat','same.png')")
        .unwrap();
    for filename in ["same.png", "nested/same.png", "nested/A.dat"] {
        std::fs::remove_file(appdata.join(filename)).unwrap();
    }
    runtime
        .execute_source("res.drawSprite('A',0,0); res.drawSprite('A',2,0)")
        .unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), OLD, OLD);
}

#[cfg(unix)]
#[test]
fn symlink_requested_extension_decodes_and_retains_pixels_after_input_deletion() {
    let files = Files::new();
    let mut image = vec![0_u8; 18];
    image[2] = 2;
    image[12..14].copy_from_slice(&2_u16.to_le_bytes());
    image[14..16].copy_from_slice(&2_u16.to_le_bytes());
    image[16] = 32;
    image[17] = 0x28;
    image.extend_from_slice(&[OLD[2], OLD[1], OLD[0], OLD[3]].repeat(4));
    std::fs::write(files.data().join("storage.bin"), image).unwrap();
    std::os::unix::fs::symlink("storage.bin", files.data().join("logical.tga")).unwrap();
    let mut payload = 1_u16.to_be_bytes().to_vec();
    string(&mut payload, "logical.tga");
    payload.extend_from_slice(&1_u16.to_be_bytes());
    string(&mut payload, "A");
    for value in [0_u16, 0, 2, 2, 0, 0] {
        payload.extend_from_slice(&value.to_be_bytes());
    }
    std::fs::write(files.data().join("A.dat"), envelope(b"SPRT", payload)).unwrap();
    let runtime = StellaLua::new_with_resolution(files.data(), 4, 2).unwrap();
    runtime
        .execute_source("res.createSpriteSheet('A.dat')")
        .unwrap();
    for filename in ["logical.tga", "storage.bin", "A.dat"] {
        std::fs::remove_file(files.data().join(filename)).unwrap();
    }
    runtime
        .execute_source("res.drawSprite('A',0,0); res.drawSprite('A',2,0)")
        .unwrap();
    assert_halves(&pixels(&runtime, &mut assets(&files)), OLD, OLD);
}
