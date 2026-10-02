//! Browser presentation of the same private native-account UI as desktop.

use crate::{
    GameResolution, ScriptResultExt,
    account_ui::{
        AccountPainter, AccountUi, Command, Field,
        keyboard::{Key, ModifiersState, NamedKey},
    },
};
use anyhow::Result;
use image::RgbaImage;
use serde_json::{Value, json};
use std::{cell::Cell, ffi::CString, path::PathBuf, time::Duration};
use stella_script::{AccountUiSnapshot, StellaLua};

pub(super) struct BrowserAccount {
    ui: AccountUi,
    painter: AccountPainter,
    snapshot: Option<AccountUiSnapshot>,
    token: u32,
    clock: f64,
    started: f64,
    pixels: Option<RgbaImage>,
    packet: CString,
    // Dedicated transient editor transfer. Never part of a render/save packet.
    editor: CString,
    external_url: Option<&'static str>,
}

thread_local! { static NEXT_TOKEN: Cell<u32> = const { Cell::new(0) }; }
fn next_token() -> u32 {
    NEXT_TOKEN.with(|value| {
        let next = value.get().wrapping_add(1).max(1);
        value.set(next);
        next
    })
}

impl BrowserAccount {
    pub(super) fn new(runtime: &StellaLua, language: &str) -> Self {
        let mut painter = AccountPainter::new(PathBuf::from("/runtime/data"), runtime);
        painter.set_languages(&[language.to_owned()]);
        Self {
            ui: AccountUi::default(),
            painter,
            snapshot: None,
            token: 0,
            clock: 0.0,
            started: 0.0,
            pixels: None,
            packet: CString::new("null").unwrap(),
            editor: CString::new("null").unwrap(),
            external_url: None,
        }
    }

    pub(super) fn set_language(&mut self, language: &str) {
        self.painter.set_languages(&[language.to_owned()]);
        self.token = next_token();
    }

    pub(super) fn sync(&mut self, runtime: &StellaLua, resolution: GameResolution) -> Result<bool> {
        let next = runtime.account_ui();
        let changed = self.snapshot.as_ref().map(|s| (s.id, s.view, s.busy))
            != next.as_ref().map(|s| (s.id, s.view, s.busy));
        let owner_changed = self.ui.sync(next.clone());
        if owner_changed {
            self.started = self.clock;
            self.editor = CString::new("null")?;
            self.external_url = None;
            self.pixels = None;
            runtime.clear_platform_input_for_modal().browser()?;
            if self.ui.visible() {
                self.ui
                    .set_calendar_today(runtime.account_calendar_today().browser()?);
            }
        }
        if changed {
            self.token = next_token();
        }
        self.snapshot = next;
        self.painter
            .synchronize_context(&self.ui, resolution.width, resolution.height);
        Ok(owner_changed)
    }

    pub(super) fn frame(
        &mut self,
        runtime: &StellaLua,
        resolution: GameResolution,
        now: f64,
        active: bool,
    ) -> Result<bool> {
        if now.is_finite() && (0.0..=1e12).contains(&now) {
            self.clock = self.clock.max(now);
        }
        let changed_owner = self.sync(runtime, resolution)?;
        self.ui
            .set_validation_clock(Duration::from_secs_f64(self.clock - self.started));
        if active {
            self.ui.advance_validation(runtime).browser()?;
            self.sync(runtime, resolution)?;
        }
        if !self.ui.visible() {
            self.pixels = None;
            self.editor = CString::new("null")?;
            self.packet = CString::new("null")?;
            return Ok(changed_owner);
        }
        let image = self.painter.paint(
            runtime,
            &self.ui,
            resolution.width,
            resolution.height,
            self.clock - self.started,
        )?;
        let repaint = image.is_some();
        if let Some(image) = image {
            self.pixels = Some(image);
        }
        let image = repaint.then(|| self.pixels.as_ref().map(|image| json!({"pointer":image.as_ptr() as usize,"length":image.len(),"width":image.width(),"height":image.height()}))).flatten();
        let snapshot = self.snapshot.as_ref().unwrap();
        let controls: Vec<_> = self.painter.controls(&self.ui).into_iter().map(|(name, rect, label)| json!({"name":name,"rect":[rect.x,rect.y,rect.width,rect.height],"label":label})).collect();
        self.packet = CString::new(serde_json::to_vec(
            &json!({"token":self.token,"view":format!("{:?}",snapshot.view),"busy":snapshot.busy,"focus":self.ui.focused().map(field_index),"resolution":[resolution.width,resolution.height],"image":image,"controls":controls,"externalUrl":self.external_url.take()}),
        )?)?;
        Ok(changed_owner)
    }

    pub(super) fn packet(&self) -> *const std::ffi::c_char {
        self.packet.as_ptr()
    }

    pub(super) fn accepts(&self, token: u32) -> bool {
        self.ui.visible() && token == self.token
    }

    fn command(&mut self, runtime: &StellaLua, command: Command) -> Result<()> {
        if let Command::OpenUrl(url) = command {
            self.external_url = Some(url);
        } else {
            self.ui.execute(runtime, command).browser()?;
        }
        Ok(())
    }

    pub(super) fn pointer(
        &mut self,
        runtime: &StellaLua,
        phase: i32,
        x: f32,
        y: f32,
        extend: bool,
    ) -> Result<()> {
        self.ui.move_pointer(x, y);
        match phase {
            0 => {
                let hit = self.painter.hit(x, y);
                self.ui.press(hit);
                if !self.ui.busy() {
                    let field = match hit {
                        Some("emailTextField") => Some(Field::Email),
                        Some("passwordTextField") => Some(Field::Password),
                        _ => None,
                    };
                    self.ui.focus(field);
                    if let Some(field) = field {
                        self.painter
                            .place_cursor(runtime, &mut self.ui, field, x, extend)?;
                    }
                }
            }
            1 => {
                let field = match self.ui.pressed() {
                    Some("emailTextField") => Some(Field::Email),
                    Some("passwordTextField") => Some(Field::Password),
                    _ => None,
                };
                if let Some(field) = field {
                    self.painter
                        .place_cursor(runtime, &mut self.ui, field, x, true)?;
                }
            }
            2 => {
                if let Some(command) = self.ui.release(self.painter.hit(x, y)) {
                    self.command(runtime, command)?;
                }
            }
            _ => self.ui.press(None),
        }
        Ok(())
    }

    pub(super) fn control(&mut self, runtime: &StellaLua, name: &str) -> Result<()> {
        if !self
            .painter
            .controls(&self.ui)
            .iter()
            .any(|(control, _, _)| *control == name)
        {
            return Ok(());
        }
        self.ui.press(Some(name));
        if let Some(command) = self.ui.release(Some(name)) {
            self.command(runtime, command)?;
        }
        Ok(())
    }

    pub(super) fn key(&mut self, runtime: &StellaLua, name: &str, shift: bool) -> Result<()> {
        let key = match name {
            "Escape" => NamedKey::Escape,
            "Enter" => NamedKey::Enter,
            "Tab" => NamedKey::Tab,
            "ArrowUp" => NamedKey::ArrowUp,
            "ArrowDown" => NamedKey::ArrowDown,
            "Home" => NamedKey::Home,
            "End" => NamedKey::End,
            _ => return Ok(()),
        };
        if let Some(command) = self.ui.key(
            &Key::Named(key),
            None,
            ModifiersState::new(shift, false, false),
        ) {
            self.command(runtime, command)?;
        }
        Ok(())
    }

    pub(super) fn wheel(&mut self, rows: i32) {
        self.ui.scroll_picker(rows);
    }
    pub(super) fn focus(&mut self, field: i32) {
        self.ui.focus(field_at(field));
    }

    pub(super) fn edit(&mut self, field: i32, value: &Value) {
        if let (Some(field), Some(text), Some(start), Some(end)) = (
            field_at(field),
            value["value"].as_str(),
            value["start"].as_u64(),
            value["end"].as_u64(),
        ) {
            self.ui.set_host_editor(
                field,
                text,
                start as usize,
                end as usize,
                value["backward"].as_bool().unwrap_or(false),
            );
        }
    }

    pub(super) fn editor(&mut self, field: i32) -> Result<*const std::ffi::c_char> {
        let value = field_at(field).and_then(|field|self.ui.host_editor(field)).map(|(text,start,end,backward)|json!({"value":text,"start":start,"end":end,"backward":backward}));
        self.editor = CString::new(serde_json::to_vec(&value)?)?;
        Ok(self.editor.as_ptr())
    }
}

fn field_index(field: Field) -> i32 {
    if field == Field::Email { 0 } else { 1 }
}
fn field_at(index: i32) -> Option<Field> {
    match index {
        0 => Some(Field::Email),
        1 => Some(Field::Password),
        _ => None,
    }
}
