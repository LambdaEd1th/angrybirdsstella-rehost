use super::*;
use stella_script::AppRatingButton;

fn prompt(id: u64) -> AppRatingPrompt {
    AppRatingPrompt {
        id,
        message: "Enjoying Angry Birds Stella? Please rate the game!".to_owned(),
        buttons: [
            AppRatingButton {
                choice: AppRatingChoice::Later,
                title: "Remind me later".to_owned(),
            },
            AppRatingButton {
                choice: AppRatingChoice::Decline,
                title: "No, thanks".to_owned(),
            },
            AppRatingButton {
                choice: AppRatingChoice::Rate,
                title: "Rate now".to_owned(),
            },
        ],
    }
}

fn point(ui: &mut AppRatingUi, choice: AppRatingChoice) {
    let rect = ui.button_rect(choice).unwrap();
    ui.move_pointer(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
}

#[test]
fn apprater_modal_press_cannot_cross_owner_resize_cancel_or_other_button() {
    let runtime = StellaLua::new(std::env::temp_dir()).unwrap();
    let mut ui = AppRatingUi::default();
    ui.sync(Some(prompt(1)), 1024, 768);
    assert!(ui.paint(&runtime, false).unwrap().is_some());
    point(&mut ui, AppRatingChoice::Rate);
    ui.press();
    point(&mut ui, AppRatingChoice::Decline);
    assert_eq!(ui.release(), None);
    point(&mut ui, AppRatingChoice::Rate);
    ui.press();
    ui.sync(Some(prompt(2)), 1024, 768);
    assert_eq!(ui.release(), None);
    ui.paint(&runtime, false).unwrap();
    point(&mut ui, AppRatingChoice::Rate);
    ui.press();
    ui.sync(Some(prompt(2)), 768, 1024);
    assert_eq!(ui.release(), None);
    let portrait = ui.paint(&runtime, false).unwrap().unwrap();
    assert_eq!(portrait.dimensions(), (768, 1024));
    point(&mut ui, AppRatingChoice::Rate);
    ui.press();
    ui.cancel_press();
    assert_eq!(ui.release(), None);
    ui.press();
    assert_eq!(ui.release(), Some((2, AppRatingChoice::Rate)));
}

#[test]
fn apprater_modal_keyboard_wraps_without_defaulting_to_rate_and_repaint_can_be_forced() {
    let runtime = StellaLua::new(std::env::temp_dir()).unwrap();
    let mut ui = AppRatingUi::default();
    ui.sync(Some(prompt(7)), 1024, 768);
    assert_eq!(ui.focused_answer(), Some((7, AppRatingChoice::Later)));
    ui.focus_next(true);
    assert_eq!(ui.focused_answer(), Some((7, AppRatingChoice::Rate)));
    ui.focus_next(false);
    assert_eq!(ui.focused_answer(), Some((7, AppRatingChoice::Later)));
    let image = ui.paint(&runtime, false).unwrap().unwrap();
    assert!(ui.paint(&runtime, false).unwrap().is_none());
    assert_eq!(ui.paint(&runtime, true).unwrap(), Some(image));
    ui.sync(None, 1024, 768);
    assert!(ui.focused_answer().is_none());
    assert!(ui.paint(&runtime, true).unwrap().is_none());
}
