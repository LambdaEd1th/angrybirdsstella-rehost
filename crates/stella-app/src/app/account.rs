//! Native-style account modal ownership and desktop input routing.

use super::*;
use crate::account_ui::{Command, Field};

impl StellaApp {
    pub(super) fn synchronize_account_ui(&mut self) -> Result<()> {
        self.synchronize_app_rating_ui()?;
        self.account_ui.sync(self.runtime.account_ui());
        let size = self.window.as_ref().map_or(
            PhysicalSize::new(self.resolution.width, self.resolution.height),
            |window| window.inner_size(),
        );
        self.account_painter
            .synchronize_context(&self.account_ui, size.width, size.height);
        let owner_changed = self.account_owner != self.account_ui.owner_id();
        if owner_changed {
            self.account_owner = self.account_ui.owner_id();
            if self.account_owner.is_some() {
                self.account_ui.set_calendar_today(
                    self.runtime
                        .account_calendar_today()
                        .map_err(|error| anyhow!(error.to_string()))?,
                );
            }
            self.runtime
                .clear_platform_input_for_modal()
                .map_err(|error| anyhow!(error.to_string()))?;
            self.reset_platform_input();
            self.account_started = Instant::now();
        }
        self.account_ui
            .set_validation_clock(self.account_started.elapsed());
        if self.active {
            self.account_ui
                .advance_validation(&self.runtime)
                .map_err(|error| anyhow!(error.to_string()))?;
            self.account_ui.sync(self.runtime.account_ui());
        }
        if self.platform_overlay.is_some()
            && !self.account_ui.visible()
            && !self.app_rating_ui.visible()
            && let Some(renderer) = &mut self.renderer
        {
            renderer.set_window_overlay(None)?;
            self.platform_overlay = None;
        }
        self.synchronize_account_ime();
        Ok(())
    }

    fn synchronize_account_ime(&mut self) {
        let focus = self.account_ui.focused();
        let enabled = self.active
            && self.account_ui.visible()
            && !self.app_rating_ui.visible()
            && !self.account_ui.busy()
            && focus.is_some();
        if let Some(window) = &self.window {
            if enabled != self.account_ime_enabled {
                window.set_ime_allowed(enabled);
                self.account_ime_enabled = enabled;
            }
            if enabled {
                window.set_ime_purpose(if focus == Some(Field::Password) {
                    winit::window::ImePurpose::Password
                } else {
                    winit::window::ImePurpose::Normal
                });
                if let Some(rect) = focus.and_then(|field| self.account_painter.ime_rect(field)) {
                    window.set_ime_cursor_area(
                        winit::dpi::PhysicalPosition::new(
                            f64::from(rect.x),
                            f64::from(rect.y + rect.height),
                        ),
                        PhysicalSize::new(rect.width.max(1.0) as u32, 1),
                    );
                }
            }
        }
    }

    pub(super) fn paint_account_overlay(&mut self, width: u32, height: u32) -> Result<()> {
        if width == 0 || height == 0 {
            return Ok(());
        }
        self.synchronize_account_ui()?;
        if self.app_rating_ui.visible() {
            if let Some(image) = self.app_rating_ui.paint(
                &self.runtime,
                self.platform_overlay != Some(PlatformOverlay::AppRating),
            )? && let Some(renderer) = &mut self.renderer
            {
                renderer.set_window_overlay(Some(&image))?;
                self.platform_overlay = Some(PlatformOverlay::AppRating);
            }
            return Ok(());
        }
        if self.account_ui.visible()
            && let Some(image) = self.account_painter.paint(
                &self.runtime,
                &self.account_ui,
                width,
                height,
                self.account_started.elapsed().as_secs_f64(),
            )?
            && let Some(renderer) = &mut self.renderer
        {
            renderer.set_window_overlay(Some(&image))?;
            self.platform_overlay = Some(PlatformOverlay::Account);
        }
        self.synchronize_account_ime();
        Ok(())
    }

    fn account_command(&mut self, command: Command) -> Result<()> {
        if let Command::OpenUrl(url) = command {
            if let Err(error) = super::platform_actions::launch_external_target(url) {
                eprintln!("platform action failed: {error}");
            }
            return Ok(());
        }
        if matches!(command, Command::Paste | Command::Copy | Command::Cut) {
            self.account_clipboard_command(command);
            return Ok(());
        }
        self.account_ui
            .execute(&self.runtime, command)
            .map_err(|error| anyhow!(error.to_string()))?;
        self.synchronize_account_ui()
    }

    fn account_clipboard_command(&mut self, command: Command) {
        if self.account_ui.focused().is_none() || self.account_ui.busy() {
            return;
        }
        let selection = if matches!(command, Command::Copy | Command::Cut) {
            let Some(selection) = self.account_ui.copyable_selection() else {
                return;
            };
            Some(selection.to_owned())
        } else {
            None
        };
        // Lazily acquire only after explicit paste/copy/cut input. In
        // particular, startup, focus, rendering and tests never read clipboard.
        if self.account_clipboard.is_none() {
            self.account_clipboard = arboard::Clipboard::new().ok();
        }
        let Some(clipboard) = &mut self.account_clipboard else {
            return;
        };
        if command == Command::Paste {
            if let Ok(text) = clipboard.get_text() {
                self.account_ui.text(&text);
            }
        } else if let Some(selection) = selection
            && clipboard.set_text(selection).is_ok()
            && command == Command::Cut
        {
            self.account_ui.text("");
        }
    }

    fn account_pointer_down(&mut self, x: f32, y: f32) -> Result<()> {
        self.account_ui.move_pointer(x, y);
        let hit = self.account_painter.hit(x, y);
        self.account_ui.press(hit);
        if self.account_ui.busy() {
            return Ok(());
        }
        let field = match hit {
            Some("emailTextField") => Some(Field::Email),
            Some("passwordTextField") => Some(Field::Password),
            _ => None,
        };
        self.account_ui.focus(field);
        if let Some(field) = field {
            self.account_painter.place_cursor(
                &self.runtime,
                &mut self.account_ui,
                field,
                x,
                self.modifiers.shift_key(),
            )?;
        }
        self.synchronize_account_ime();
        Ok(())
    }

    fn account_pointer_move(&mut self, x: f32, y: f32) -> Result<()> {
        self.account_ui.move_pointer(x, y);
        let field = match self.account_ui.pressed() {
            Some("emailTextField") => Some(Field::Email),
            Some("passwordTextField") => Some(Field::Password),
            _ => None,
        };
        if let Some(field) = field {
            self.account_painter.place_cursor(
                &self.runtime,
                &mut self.account_ui,
                field,
                x,
                true,
            )?;
        }
        Ok(())
    }

    fn account_pointer_up(&mut self, x: f32, y: f32) -> Result<()> {
        if let Some(command) = self.account_ui.release(self.account_painter.hit(x, y)) {
            self.account_command(command)?;
        }
        Ok(())
    }

    /// Consume all modal pointer/key events, including button-up and wheel.
    /// Window lifecycle/resize/redraw still reach the normal app handler.
    pub(super) fn account_window_event(&mut self, event: &WindowEvent) -> Result<bool> {
        self.synchronize_account_ui()?;
        if !self.account_ui.visible() {
            return Ok(false);
        }
        match event {
            WindowEvent::CursorMoved { position, .. } => {
                self.account_pointer_move(position.x as f32, position.y as f32)?
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if *button == MouseButton::Left {
                    let [x, y] = self.account_ui.pointer();
                    if *state == ElementState::Pressed {
                        self.account_pointer_down(x, y)?;
                    } else {
                        self.account_pointer_up(x, y)?;
                    }
                }
            }
            WindowEvent::Touch(touch) => {
                let (x, y) = (touch.location.x as f32, touch.location.y as f32);
                match touch.phase {
                    TouchPhase::Started if self.primary_touch.is_none() => {
                        self.primary_touch = Some(touch.id);
                        self.account_pointer_down(x, y)?;
                    }
                    TouchPhase::Moved if self.primary_touch == Some(touch.id) => {
                        self.account_pointer_move(x, y)?
                    }
                    TouchPhase::Ended if self.primary_touch == Some(touch.id) => {
                        self.primary_touch = None;
                        self.account_pointer_up(x, y)?;
                    }
                    TouchPhase::Cancelled if self.primary_touch == Some(touch.id) => {
                        self.primary_touch = None;
                        self.account_ui.press(None);
                    }
                    _ => {}
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed
                    && let Some(command) = self.account_ui.key(
                        &event.logical_key,
                        event.text.as_deref(),
                        self.modifiers,
                    )
                {
                    self.account_command(command)?;
                }
                self.synchronize_account_ime();
            }
            WindowEvent::Ime(event) => match event {
                winit::event::Ime::Commit(text) => self.account_ui.text(text),
                winit::event::Ime::Preedit(text, cursor) => self.account_ui.preedit(text, *cursor),
                winit::event::Ime::Disabled => self.account_ui.clear_preedit(),
                winit::event::Ime::Enabled => {}
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let rows = match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => -*y,
                    winit::event::MouseScrollDelta::PixelDelta(position) => {
                        -position.y as f32 / 24.0
                    }
                };
                self.account_ui.scroll_picker(rows.round() as i32);
            }
            WindowEvent::Focused(false) => {
                self.account_ui.clear_preedit();
                self.account_ui.press(None);
                return Ok(false);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }
}

#[cfg(test)]
mod tests;
