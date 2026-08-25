//! winit application lifecycle and window event dispatch.

use super::*;

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
                if let Err(error) = self
                    .runtime
                    .set_screen_resolution(resolution.width, resolution.height)
                {
                    self.fatal_error = Some(error.to_string());
                    self.window = Some(window);
                    return;
                }
                self.resolution = resolution;
                match GpuRenderer::for_window(Arc::clone(&window), resolution) {
                    Ok(renderer) => {
                        self.renderer = Some(renderer);
                        match AudioDevice::open() {
                            Ok(audio) => self.audio = Some(audio),
                            Err(error) => eprintln!("audio output unavailable: {error}"),
                        }
                        self.window = Some(window);
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
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
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
                if let Ok(resolution) = GameResolution::new(size.width, size.height)
                    && resolution != self.resolution
                {
                    match self
                        .runtime
                        .set_screen_resolution(resolution.width, resolution.height)
                    {
                        Ok(_) => {
                            self.resolution = resolution;
                            if let Some(renderer) = &mut self.renderer {
                                renderer.resize_game_target(resolution);
                            }
                        }
                        Err(error) => self.fatal_error = Some(error.to_string()),
                    }
                }
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize_surface(size.width, size.height);
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
        if let Some(error) = self.fatal_error.take() {
            eprintln!("runtime stopped: {error}");
            event_loop.exit();
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.will_resign_active();
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
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
