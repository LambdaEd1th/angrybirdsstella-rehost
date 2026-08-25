//! Cross-platform host half of Purple's screenshot/share member.

use super::*;

/// Materialize the framebuffer files that Purple hands to its platform share
/// service. A pure Rust desktop host has no portable system share sheet, so
/// these files remain in the platform temporary directory for the user or a
/// future host integration instead of running the native completion callback
/// that deletes them.
pub(super) fn stage_screenshot_shares(
    requests: &[ScreenshotShareRequest],
    rgba: &[u8],
    resolution: GameResolution,
) -> Result<Vec<PathBuf>> {
    let expected_len = u64::from(resolution.width)
        .checked_mul(u64::from(resolution.height))
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .ok_or_else(|| anyhow!("screenshot dimensions overflow host address space"))?;
    if rgba.len() != expected_len {
        return Err(anyhow!(
            "screenshot buffer has {} bytes, expected {expected_len}",
            rgba.len()
        ));
    }

    let mut paths = Vec::with_capacity(requests.len());
    for request in requests {
        let destination = std::env::temp_dir().join(&request.filename);
        image::save_buffer(
            &destination,
            rgba,
            resolution.width,
            resolution.height,
            image::ColorType::Rgba8,
        )
        .with_context(|| format!("save shared screenshot {}", destination.display()))?;
        eprintln!(
            "screenshot ready for sharing: {} ({})",
            destination.display(),
            request.title
        );
        paths.push(destination);
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_native_filename_as_an_rgba_png() {
        let sequence = i32::try_from(std::process::id()).unwrap();
        let filename = format!("Stella_Screenshot{sequence}.png");
        let request = ScreenshotShareRequest {
            sequence,
            filename: filename.clone(),
            title: "share title".to_owned(),
        };
        let rgba = [255, 0, 0, 255, 0, 255, 0, 255];
        let paths = stage_screenshot_shares(
            &[request],
            &rgba,
            GameResolution {
                width: 2,
                height: 1,
            },
        )
        .unwrap();

        assert_eq!(paths, vec![std::env::temp_dir().join(filename)]);
        let decoded = image::open(&paths[0]).unwrap().to_rgba8();
        assert_eq!(decoded.as_raw(), &rgba);
        fs::remove_file(&paths[0]).unwrap();
    }

    #[test]
    fn rejects_a_framebuffer_with_the_wrong_extent() {
        let request = ScreenshotShareRequest {
            sequence: 1,
            filename: "Stella_Screenshot1.png".to_owned(),
            title: String::new(),
        };
        let error = stage_screenshot_shares(
            &[request],
            &[0; 7],
            GameResolution {
                width: 2,
                height: 1,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("expected 8"));
    }
}
