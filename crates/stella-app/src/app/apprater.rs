//! Rating alert intercepts input above the game and any account form.

use super::*;
use stella_script::AppRatingChoice;

impl StellaApp {
    pub(super) fn synchronize_app_rating_ui(&mut self) -> Result<()> {
        let size = self.window.as_ref().map_or(
            PhysicalSize::new(self.resolution.width, self.resolution.height),
            |window| window.inner_size(),
        );
        if self
            .app_rating_ui
            .sync(self.runtime.app_rating_prompt(), size.width, size.height)
        {
            self.runtime
                .clear_platform_input_for_modal()
                .map_err(|error| anyhow!(error.to_string()))?;
            self.reset_platform_input();
            self.account_ui.press(None);
            self.account_painter.invalidate();
        }
        Ok(())
    }

    fn answer_app_rating(&mut self, answer: Option<(u64, AppRatingChoice)>) -> Result<()> {
        if let Some((id, choice)) = answer {
            self.runtime
                .answer_app_rating(id, choice)
                .map_err(|error| anyhow!(error.to_string()))?;
            self.synchronize_account_ui()?;
        }
        Ok(())
    }

    pub(super) fn app_rating_window_event(&mut self, event: &WindowEvent) -> Result<bool> {
        self.synchronize_app_rating_ui()?;
        if !self.app_rating_ui.visible() {
            return Ok(false);
        }
        match event {
            WindowEvent::CursorMoved { position, .. } => self
                .app_rating_ui
                .move_pointer(position.x as f32, position.y as f32),
            WindowEvent::MouseInput { state, button, .. } => {
                if *button == MouseButton::Left {
                    if *state == ElementState::Pressed {
                        self.app_rating_ui.press();
                    } else {
                        let answer = self.app_rating_ui.release();
                        self.answer_app_rating(answer)?;
                    }
                }
            }
            WindowEvent::Touch(touch) => {
                let (x, y) = (touch.location.x as f32, touch.location.y as f32);
                match touch.phase {
                    TouchPhase::Started if self.primary_touch.is_none() => {
                        self.primary_touch = Some(touch.id);
                        self.app_rating_ui.move_pointer(x, y);
                        self.app_rating_ui.press();
                    }
                    TouchPhase::Moved if self.primary_touch == Some(touch.id) => {
                        self.app_rating_ui.move_pointer(x, y)
                    }
                    TouchPhase::Ended if self.primary_touch == Some(touch.id) => {
                        self.primary_touch = None;
                        self.app_rating_ui.move_pointer(x, y);
                        let answer = self.app_rating_ui.release();
                        self.answer_app_rating(answer)?;
                    }
                    TouchPhase::Cancelled if self.primary_touch == Some(touch.id) => {
                        self.primary_touch = None;
                        self.app_rating_ui.cancel_press();
                    }
                    _ => {}
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed && !event.repeat {
                    match &event.logical_key {
                        Key::Named(NamedKey::Tab) => {
                            self.app_rating_ui.focus_next(self.modifiers.shift_key())
                        }
                        Key::Named(NamedKey::ArrowUp | NamedKey::ArrowLeft) => {
                            self.app_rating_ui.focus_next(true)
                        }
                        Key::Named(NamedKey::ArrowDown | NamedKey::ArrowRight) => {
                            self.app_rating_ui.focus_next(false)
                        }
                        Key::Named(NamedKey::Enter | NamedKey::Space) => {
                            self.answer_app_rating(self.app_rating_ui.focused_answer())?
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::MouseWheel { .. } | WindowEvent::Ime(_) => {}
            WindowEvent::Focused(false) => {
                self.app_rating_ui.cancel_press();
                return Ok(false);
            }
            _ => return Ok(false),
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
        Ok(true)
    }
}
