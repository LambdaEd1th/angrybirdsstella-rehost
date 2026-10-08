//! winit application lifecycle and window event dispatch.

use super::*;

/// Keep fatal errors through `exiting` so a GUI launcher can report them after
/// winit has completed the native termination/persistence callback boundary.
#[derive(Default)]
pub(super) struct WindowErrors(Vec<String>);

impl WindowErrors {
    fn record(&mut self, error: String) {
        self.0.push(error);
    }

    pub(super) fn finish(&mut self, result: Result<()>) -> Result<()> {
        if self.0.is_empty() {
            return result;
        }
        let errors = std::mem::take(&mut self.0).join("\n");
        match result {
            Ok(()) => Err(anyhow!(errors)),
            Err(error) => Err(error.context(errors)),
        }
    }
}

impl StellaApp {
    pub(crate) fn finish_window_run(&mut self, result: Result<()>) -> Result<()> {
        self.window_errors.finish(result)
    }

    pub(super) fn terminate_window(&mut self) {
        // Linux clipboard ownership lives with this handle; winit may finish
        // the event loop without dropping the app value on every platform.
        self.account_clipboard = None;
        self.application_will_terminate();
        if let Some(error) = self.fatal_error.take() {
            eprintln!("runtime stopped during termination: {error}");
            self.window_errors.record(error);
        }
    }
}

impl ApplicationHandler for StellaApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            self.did_become_active();
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("Angry Birds Stella: Rehost")
            .with_inner_size(PhysicalSize::new(
                self.resolution.width,
                self.resolution.height,
            ))
            .with_min_inner_size(PhysicalSize::new(512, 384));
        match event_loop.create_window(attributes) {
            Ok(window) => {
                let window = Arc::new(window);
                let size = window.inner_size();
                let resolution =
                    GameResolution::new(size.width, size.height).unwrap_or(self.resolution);
                match GpuRenderer::for_window(Arc::clone(&window), resolution) {
                    Ok(renderer) => {
                        // IOSOSInterface installs the new gr::Context extent
                        // before GameApp slot 7 publishes screenWidth/Height
                        // and calls Lua resolutionChanged. Make the real wgpu
                        // target authoritative before the same notification.
                        self.renderer = Some(renderer);
                        self.rendered_frame_ready = false;
                        self.window = Some(window);
                        self.resolution = resolution;
                        if let Err(error) = self
                            .runtime
                            .set_screen_resolution(resolution.width, resolution.height)
                        {
                            self.fatal_error = Some(error.to_string());
                            return;
                        }
                        match AudioDevice::open() {
                            Ok(audio) => self.audio = Some(audio),
                            Err(error) => eprintln!("audio output unavailable: {error}"),
                        }
                        self.did_become_active();
                    }
                    Err(error) => self.fatal_error = Some(error.to_string()),
                }
            }
            Err(error) => self.fatal_error = Some(error.to_string()),
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self
            .window
            .as_ref()
            .is_none_or(|window| window.id() != window_id)
        {
            return;
        }
        let consumed = match self.app_rating_window_event(&event).and_then(|consumed| {
            if consumed {
                Ok(true)
            } else {
                self.account_window_event(&event)
            }
        }) {
            Ok(consumed) => consumed,
            Err(error) => {
                self.fatal_error = Some(error.to_string());
                true
            }
        };
        if !consumed {
            match event {
                WindowEvent::CloseRequested => {
                    // Purple's applicationWillTerminate: (0x1004047A8)
                    // stops updates and delivers the final pause/save pass
                    // without consulting GameLua's safe-to-quit byte. The
                    // shipped iOS scripts can leave that byte false forever.
                    // Let winit reach `exiting`, which preserves that final
                    // callback and reports any persistence failure.
                    event_loop.exit();
                }
                WindowEvent::Focused(focused) => {
                    if focused {
                        self.did_become_active();
                    } else {
                        self.will_resign_active();
                    }
                }
                WindowEvent::CursorMoved { position, .. } => {
                    self.update_cursor(position.x, position.y);
                }
                WindowEvent::MouseInput {
                    state,
                    button: MouseButton::Left,
                    ..
                } => {
                    self.cursor_down = state == ElementState::Pressed;
                    if let Err(error) =
                        self.runtime
                            .set_cursor(self.cursor.0, self.cursor.1, self.cursor_down)
                    {
                        self.fatal_error = Some(error.to_string());
                    }
                }
                WindowEvent::Touch(touch) => {
                    self.update_touch(touch.id, touch.phase, touch.location.x, touch.location.y);
                }
                WindowEvent::MouseWheel { delta, .. } => self.update_mouse_wheel(delta),
                WindowEvent::KeyboardInput { event, .. } => self.update_keyboard(event),
                WindowEvent::ModifiersChanged(modifiers) => {
                    self.modifiers = modifiers.state();
                }
                WindowEvent::Resized(size) => {
                    // MyEAGLView recreates its drawable and GL_Context applies
                    // the new backing extent before GameApp delivers the Lua
                    // resolution notification. Keep both wgpu targets on that
                    // side of the callback as well.
                    if let Some(renderer) = &mut self.renderer
                        && let Err(error) = renderer.resize_surface(size.width, size.height)
                    {
                        self.fatal_error = Some(error.to_string());
                    } else if let Ok(resolution) = GameResolution::new(size.width, size.height)
                        && resolution != self.resolution
                        && let Err(error) = self.resize_runtime_target(resolution)
                    {
                        self.fatal_error = Some(error.to_string());
                    }
                    if let Some(window) = &self.window {
                        window.request_redraw();
                    }
                }
                WindowEvent::RedrawRequested => {
                    if let Err(error) = self.render() {
                        self.fatal_error = Some(error.to_string());
                    }
                }
                _ => {}
            }
        }
        if let Err(error) = self.synchronize_account_ui() {
            self.fatal_error = Some(error.to_string());
        }
        if let Some(error) = self.fatal_error.take() {
            eprintln!("runtime stopped: {error}");
            self.window_errors.record(error);
            event_loop.exit();
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.will_resign_active();
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.terminate_window();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.active {
            self.advance();
        }
        if let Some(error) = self.fatal_error.take() {
            eprintln!("runtime stopped: {error}");
            self.window_errors.record(error);
            event_loop.exit();
            return;
        }
        if self.runtime.exit_requested() {
            event_loop.exit();
            return;
        }
        if self.active
            && let Some(window) = &self.window
        {
            window.request_redraw();
        }
        event_loop.set_control_flow(if self.active {
            ControlFlow::WaitUntil(super::runtime::next_display_link_deadline(
                self.last_tick,
                self.accumulator,
            ))
        } else {
            ControlFlow::Wait
        });
    }
}

#[cfg(test)]
mod tests {
    use super::WindowErrors;

    #[test]
    fn window_errors_survive_normal_loop_exit_and_termination_errors() {
        let mut errors = WindowErrors::default();
        errors.record("GPU device lost".into());
        errors.record("save persistence failed".into());
        assert_eq!(
            errors.finish(Ok(())).unwrap_err().to_string(),
            "GPU device lost\nsave persistence failed"
        );
    }

    #[test]
    fn window_errors_preserve_an_event_loop_error_too() {
        let mut errors = WindowErrors::default();
        errors.record("save persistence failed".into());
        let result = errors
            .finish(Err(anyhow::anyhow!("event loop failed")))
            .unwrap_err();
        assert!(format!("{result:#}").contains("event loop failed"));
        assert!(result.to_string().contains("save persistence failed"));
        assert!(WindowErrors::default().finish(Ok(())).is_ok());
    }
}
