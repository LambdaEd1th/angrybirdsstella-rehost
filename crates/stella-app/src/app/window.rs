//! winit application lifecycle and window event dispatch.

use super::*;

/// Desktop delivery of GameApp's native `isSafeToQuit` virtual.
///
/// Purple retains a Lua-derived byte at `GameLua+0x6AC` and exposes it through
/// the GameApp vtable. A platform close request made while that byte is false
/// remains pending until a later frame publishes true. Script-requested exits
/// are a separate force path and deliberately bypass this gate.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct CloseRequest {
    pending: bool,
}

impl CloseRequest {
    fn request(&mut self, safe_to_quit: bool) -> bool {
        self.pending = true;
        safe_to_quit
    }

    fn should_exit(self, safe_to_quit: bool, forced: bool) -> bool {
        forced || (self.pending && safe_to_quit)
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
                    if self.close_request.request(self.runtime.safe_to_quit()) {
                        event_loop.exit();
                    }
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
                    if let Some(renderer) = &mut self.renderer {
                        renderer.resize_surface(size.width, size.height);
                    }
                    if let Ok(resolution) = GameResolution::new(size.width, size.height)
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
            event_loop.exit();
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.will_resign_active();
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // Linux clipboard ownership lives with this handle; winit may finish
        // the event loop without dropping the app value on every platform.
        self.account_clipboard = None;
        self.application_will_terminate();
        if let Some(error) = self.fatal_error.take() {
            eprintln!("runtime stopped during termination: {error}");
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.active {
            self.advance();
        }
        if let Some(error) = self.fatal_error.take() {
            eprintln!("runtime stopped: {error}");
            event_loop.exit();
            return;
        }
        if self
            .close_request
            .should_exit(self.runtime.safe_to_quit(), self.runtime.exit_requested())
        {
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
    use super::CloseRequest;

    #[test]
    fn unsafe_platform_close_waits_until_a_later_safe_frame() {
        let mut close = CloseRequest::default();

        assert!(!close.request(false));
        assert!(!close.should_exit(false, false));
        assert!(close.should_exit(true, false));
    }

    #[test]
    fn safe_platform_close_exits_immediately() {
        let mut close = CloseRequest::default();

        assert!(close.request(true));
        assert!(close.should_exit(true, false));
    }

    #[test]
    fn script_requested_exit_bypasses_an_unsafe_platform_gate() {
        let close = CloseRequest::default();

        assert!(close.should_exit(false, true));
    }
}
