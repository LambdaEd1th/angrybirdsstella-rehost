use super::*;

#[allow(dead_code)]
pub(super) fn blit_game(source: &[u32], target: &mut [u32], width: u32, height: u32) {
    target.fill(0);
    if width == 0 || height == 0 {
        return;
    }
    let scale = (width as f64 / GAME_WIDTH as f64).min(height as f64 / GAME_HEIGHT as f64);
    let viewport_width = (GAME_WIDTH as f64 * scale).round() as u32;
    let viewport_height = (GAME_HEIGHT as f64 * scale).round() as u32;
    let left = (width - viewport_width) / 2;
    let top = (height - viewport_height) / 2;
    for output_y in 0..viewport_height {
        let source_y = ((output_y as u64 * GAME_HEIGHT as u64) / viewport_height as u64) as u32;
        for output_x in 0..viewport_width {
            let source_x = ((output_x as u64 * GAME_WIDTH as u64) / viewport_width as u64) as u32;
            let index = ((top + output_y) * width + left + output_x) as usize;
            target[index] = source[(source_y * GAME_WIDTH + source_x) as usize];
        }
    }
}
