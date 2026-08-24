//! Letterboxed pointer mapping and native cursor bridge.

use super::*;

impl StellaApp {
    pub(super) fn view_did_disappear(&mut self) {
        let had_primary_pointer = self.primary_touch.take().is_some() || self.cursor_down;
        self.touches.clear();
        self.cursor_down = false;
        self.modifiers = ModifiersState::empty();
        if let Err(error) = self.runtime.view_did_disappear(had_primary_pointer) {
            self.fatal_error = Some(error.to_string());
        }
    }

    pub(super) fn update_keyboard(&mut self, event: winit::event::KeyEvent) {
        let key_name = match event.logical_key {
            Key::Named(NamedKey::Escape) => "KEY_BACK",
            Key::Named(NamedKey::ContextMenu) => "KEY_MENU",
            Key::Named(NamedKey::AudioVolumeUp) => "VOLUME_UP",
            Key::Named(NamedKey::AudioVolumeDown) => "VOLUME_DOWN",
            _ => return,
        };
        if let Err(error) = self
            .runtime
            .set_key(key_name, event.state == ElementState::Pressed)
        {
            self.fatal_error = Some(error.to_string());
        }
    }

    pub(super) fn update_mouse_wheel(&mut self, delta: MouseScrollDelta) {
        let vertical = match delta {
            MouseScrollDelta::LineDelta(_, y) => y,
            // The shared engine callback consumes an integer. Preserve a
            // high-resolution wheel's direction when its pixel delta is less
            // than one native unit instead of silently turning it into zero.
            MouseScrollDelta::PixelDelta(position) => {
                let y = position.y as f32;
                if y != 0.0 && y.abs() < 1.0 {
                    y.signum()
                } else {
                    y
                }
            }
        };
        let wheel_delta = vertical as i32;
        if let Err(error) = self.runtime.mouse_wheel(
            wheel_delta,
            self.modifiers.shift_key(),
            self.modifiers.control_key(),
        ) {
            self.fatal_error = Some(error.to_string());
        }
    }

    pub(super) fn update_cursor(&mut self, window_x: f64, window_y: f64) {
        let Some(window) = &self.window else { return };
        let size = window.inner_size();
        let (x, y) =
            map_window_to_game(window_x, window_y, size.width, size.height, self.resolution);
        self.cursor = (x, y);
        if let Err(error) = self.runtime.set_cursor(x, y, self.cursor_down) {
            self.fatal_error = Some(error.to_string());
        }
    }

    pub(super) fn update_touch(
        &mut self,
        id: u64,
        phase: TouchPhase,
        window_x: f64,
        window_y: f64,
    ) {
        let Some(window) = &self.window else { return };
        let size = window.inner_size();
        let (x, y) =
            map_window_to_game(window_x, window_y, size.width, size.height, self.resolution);
        // MyEAGLViewController multiplies by contentScaleFactor and then uses
        // FCVTZS before constructing framework::TouchEvent.
        let point = (id, x as i32, y as i32);
        let cursor_x = f64::from(point.1);
        let cursor_y = f64::from(point.2);
        update_native_touch_vector(&mut self.touches, phase, point);
        match phase {
            TouchPhase::Started => {
                if self.primary_touch.is_none() {
                    self.primary_touch = Some(id);
                    self.cursor = (cursor_x, cursor_y);
                    self.cursor_down = true;
                    if let Err(error) = self.runtime.set_cursor(cursor_x, cursor_y, true) {
                        self.fatal_error = Some(error.to_string());
                    }
                }
            }
            TouchPhase::Moved => {
                if self.primary_touch == Some(id) {
                    self.cursor = (cursor_x, cursor_y);
                    if let Err(error) = self.runtime.set_cursor(cursor_x, cursor_y, true) {
                        self.fatal_error = Some(error.to_string());
                    }
                }
            }
            TouchPhase::Ended | TouchPhase::Cancelled => {
                if self.primary_touch == Some(id) {
                    self.primary_touch = None;
                    self.cursor = (cursor_x, cursor_y);
                    self.cursor_down = false;
                    if let Err(error) = self.runtime.set_cursor(cursor_x, cursor_y, false) {
                        self.fatal_error = Some(error.to_string());
                    }
                }
            }
        }
        if let Err(error) = self.runtime.set_touches(&self.touches) {
            self.fatal_error = Some(error.to_string());
        }
    }
}

fn update_native_touch_vector(
    touches: &mut Vec<(u64, i32, i32)>,
    phase: TouchPhase,
    point: (u64, i32, i32),
) {
    match phase {
        // GameApp slot +0x60 appends one complete 16-byte TouchEvent.
        TouchPhase::Started => touches.push(point),
        // Slot +0x68 replaces only the first matching id.
        TouchPhase::Moved => {
            if let Some(index) = touches.iter().position(|touch| touch.0 == point.0) {
                touches[index] = point;
            }
        }
        // Slot +0x70 compacts the vector and removes every matching id,
        // including malformed duplicate begin events.
        TouchPhase::Ended | TouchPhase::Cancelled => {
            touches.retain(|touch| touch.0 != point.0);
        }
    }
}

pub(crate) fn map_window_to_game(
    x: f64,
    y: f64,
    width: u32,
    height: u32,
    resolution: GameResolution,
) -> (f64, f64) {
    if width == 0 || height == 0 {
        return (0.0, 0.0);
    }
    let scale =
        (width as f64 / resolution.width as f64).min(height as f64 / resolution.height as f64);
    let viewport_width = resolution.width as f64 * scale;
    let viewport_height = resolution.height as f64 * scale;
    let left = (width as f64 - viewport_width) * 0.5;
    let top = (height as f64 - viewport_height) * 0.5;
    (
        ((x - left) / scale).clamp(0.0, resolution.width as f64),
        ((y - top) / scale).clamp(0.0, resolution.height as f64),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_touch_vector_appends_replaces_first_and_erases_all_matches() {
        let mut touches = Vec::new();
        update_native_touch_vector(&mut touches, TouchPhase::Started, (7, 10, 20));
        update_native_touch_vector(&mut touches, TouchPhase::Started, (7, 30, 40));
        update_native_touch_vector(&mut touches, TouchPhase::Started, (8, 50, 60));
        assert_eq!(touches, [(7, 10, 20), (7, 30, 40), (8, 50, 60)]);

        update_native_touch_vector(&mut touches, TouchPhase::Moved, (7, 70, 80));
        assert_eq!(touches, [(7, 70, 80), (7, 30, 40), (8, 50, 60)]);

        update_native_touch_vector(&mut touches, TouchPhase::Ended, (7, 90, 100));
        assert_eq!(touches, [(8, 50, 60)]);
    }
}
