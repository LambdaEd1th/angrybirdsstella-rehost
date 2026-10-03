//! Private browser presentation of Purple's retained three-button alert.

use crate::{GameResolution, ScriptResultExt, apprater_ui::AppRatingUi};
use anyhow::Result;
use image::RgbaImage;
use serde_json::json;
use std::{cell::Cell, ffi::CString};
use stella_script::{AppRatingChoice, AppRatingPrompt, StellaLua};

#[derive(Default)]
pub(super) struct BrowserRating {
    ui: AppRatingUi,
    prompt: Option<AppRatingPrompt>,
    resolution: Option<GameResolution>,
    token: u32,
    pixels: Option<RgbaImage>,
    packet: Option<CString>,
}

// The runtime's prompt IDs restart with a new game. Ingress tokens survive
// cached-module restarts so an old DOM event cannot answer a new prompt.
thread_local! { static NEXT_TOKEN: Cell<u32> = const { Cell::new(0) }; }
fn next_token() -> u32 {
    NEXT_TOKEN.with(|slot| {
        let next = slot.get().wrapping_add(1).max(1);
        slot.set(next);
        next
    })
}

pub(super) fn choice(code: i32) -> Option<AppRatingChoice> {
    match code {
        0 => Some(AppRatingChoice::Rate),
        1 => Some(AppRatingChoice::Decline),
        2 => Some(AppRatingChoice::Later),
        _ => None,
    }
}

fn code(choice: AppRatingChoice) -> i32 {
    match choice {
        AppRatingChoice::Rate => 0,
        AppRatingChoice::Decline => 1,
        AppRatingChoice::Later => 2,
    }
}

impl BrowserRating {
    pub(super) fn sync(&mut self, runtime: &StellaLua, resolution: GameResolution) -> bool {
        let prompt = runtime.app_rating_prompt();
        let changed = self.prompt != prompt;
        if changed || self.resolution != Some(resolution) {
            self.token = next_token();
            self.pixels = None;
        }
        self.ui
            .sync(prompt.clone(), resolution.width, resolution.height);
        self.prompt = prompt;
        self.resolution = Some(resolution);
        changed
    }

    pub(super) fn cancel_pointer(&mut self) {
        self.ui.cancel_press();
        self.token = next_token();
    }

    pub(super) fn accepts(&self, token: u32) -> bool {
        self.ui.visible() && self.token == token
    }

    pub(super) fn frame(&mut self, runtime: &StellaLua) -> Result<()> {
        let Some(prompt) = &self.prompt else {
            self.pixels = None;
            self.packet = Some(CString::new("null")?);
            return Ok(());
        };
        let image = self.ui.paint(runtime, false)?;
        let repaint = image.is_some();
        if let Some(image) = image {
            self.pixels = Some(image);
        }
        let image = if repaint {
            self.pixels.as_ref().map(|image| {
                json!({"pointer":image.as_ptr() as usize,"length":image.len(),"width":image.width(),"height":image.height()})
            })
        } else {
            None
        };
        let buttons: Vec<_> = self
            .ui
            .button_regions()
            .iter()
            .map(|(choice, rect)| {
                let title = &prompt.buttons.iter().find(|b| b.choice == *choice).unwrap().title;
                json!({"choice":code(*choice),"title":title,"rect":[rect.x,rect.y,rect.width,rect.height]})
            })
            .collect();
        let resolution = self.resolution.unwrap();
        self.packet = Some(CString::new(serde_json::to_vec(&json!({
            "token":self.token,"message":prompt.message,"buttons":buttons,
            "focus":self.ui.focused_answer().map(|(_,choice)|code(choice)),
            "resolution":[resolution.width,resolution.height],"image":image
        }))?)?);
        Ok(())
    }

    pub(super) fn packet(&self) -> *const std::ffi::c_char {
        self.packet
            .as_ref()
            .map_or(std::ptr::null(), |s| s.as_ptr())
    }

    fn answer(
        &mut self,
        runtime: &StellaLua,
        answer: Option<(u64, AppRatingChoice)>,
    ) -> Result<()> {
        if let Some((id, choice)) = answer {
            runtime.answer_app_rating(id, choice).browser()?;
        }
        Ok(())
    }

    pub(super) fn choose(&mut self, runtime: &StellaLua, choice: AppRatingChoice) -> Result<()> {
        let answer = self.prompt.as_ref().map(|p| (p.id, choice));
        self.answer(runtime, answer)
    }

    pub(super) fn pointer(
        &mut self,
        runtime: &StellaLua,
        phase: i32,
        x: f32,
        y: f32,
    ) -> Result<()> {
        self.ui.move_pointer(x, y);
        match phase {
            0 => self.ui.press(),
            1 => {}
            2 => {
                let answer = self.ui.release();
                self.answer(runtime, answer)?;
            }
            _ => self.ui.cancel_press(),
        }
        Ok(())
    }

    pub(super) fn focus(&mut self, choice: AppRatingChoice) {
        for _ in 0..3 {
            if self.ui.focused_answer().is_some_and(|(_, c)| c == choice) {
                break;
            }
            self.ui.focus_next(false);
        }
    }

    pub(super) fn key(&mut self, runtime: &StellaLua, code: i32, shift: bool) -> Result<()> {
        match code {
            0 => self.ui.focus_next(shift),
            1 => self.ui.focus_next(true),
            2 => self.ui.focus_next(false),
            3 => self.answer(runtime, self.ui.focused_answer())?,
            // UIKit supplies only the three button choices. Escape never
            // silently turns dismissal or focus loss into a Later answer.
            _ => {}
        }
        Ok(())
    }
}
