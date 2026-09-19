//! Registration nibs plus awakeFromNib/SHVBox changes recovered from Purple ARM64.
//! See docs/native-account-registration-layout.md for sources and host boundaries.

use super::{
    AccountView, BACK_DOWN, BACK_IMAGE, BG, DARK, Element, GREY, Kind, LOGO, OK_DOWN, OK_IMAGE,
    RED, Rect, SUBMIT_DOWN, SUBMIT_IMAGE, ViewLayout, cap, error_icon, field, layout,
};

pub(crate) const DATE_IMAGE: &str =
    "skynestdata/images/identity/milkshake_datemonthyear_button.png";
pub(crate) const DATE_OPEN_IMAGE: &str =
    "skynestdata/images/identity/milkshake_datemonthyear_button_up.png";
pub(crate) const GENDER_OFF_IMAGE: &str =
    "skynestdata/images/identity/milkshake_gender_button_off.png";
pub(crate) const GENDER_ON_IMAGE: &str =
    "skynestdata/images/identity/milkshake_gender_button_on.png";
const CLOSE_IMAGE: &str = "skynestdata/images/identity/button_close_topright.png";
const CLOSE_DOWN: &str = "skynestdata/images/identity/button_close_topright_down.png";

/// UITextFields are disabled in the nib: gesture recognizers open UIPickerViews.
/// Keep these controls out of the keyboard editor, despite their outlet names.
const fn date_button(
    name: &'static str,
    rect: Rect,
    key: &'static str,
    fallback: &'static str,
) -> Element {
    Element {
        kind: Kind::Button,
        image: Some(DATE_IMAGE),
        font_name: ".HelveticaNeueInterface-Regular",
        ..Element::text(name, rect, key, fallback, 21.0)
    }
}

const REGISTER1_TITLE: [Element; 2] = [
    Element {
        font_name: "OpenSans-CondensedBold",
        alignment: 2,
        color: [191, 6, 29, 255],
        ..Element::text(
            "registerLabel",
            Rect::new(371.0, 264.0, 218.0, 42.0),
            "rovio_id_sign_up_title",
            "REGISTER",
            42.0,
        )
    },
    Element {
        text_key: None,
        ..Element::text(
            "screenNumberLabel",
            Rect::new(597.0, 278.0, 33.0, 23.0),
            "",
            "1/2",
            19.0,
        )
    },
];
const TERMS: [Element; 3] = [
    Element {
        alignment: 2,
        color: RED,
        ..Element::text(
            "eulaLabel",
            Rect::new(280.0, 451.0, 167.0, 26.0),
            "rovio_id_terms_of_service",
            "Terms of Service",
            21.0,
        )
    },
    Element {
        alignment: 1,
        color: GREY,
        ..Element::text(
            "andLabel",
            Rect::new(494.0, 451.0, 38.0, 26.0),
            "rovio_id_and",
            " and",
            21.0,
        )
    },
    Element {
        color: RED,
        ..Element::text(
            "privacyPolicyLabel",
            Rect::new(567.0, 451.0, 138.0, 26.0),
            "rovio_id_privacy_policy",
            " Privacy Policy.",
            21.0,
        )
    },
];
const REGISTER2_TITLE: [Element; 2] = [
    Element {
        font_name: "OpenSans-CondensedBold",
        alignment: 2,
        color: [191, 6, 29, 255],
        ..Element::text(
            "RegisterViewlabel",
            Rect::new(370.0, 264.0, 217.0, 42.0),
            "rovio_id_sign_up_title",
            "REGISTER",
            42.0,
        )
    },
    Element {
        text_key: None,
        ..Element::text(
            "screenNumberlabel",
            Rect::new(595.0, 278.0, 36.0, 23.0),
            "",
            "2/2",
            19.0,
        )
    },
];
const GENDER: [Element; 5] = [
    Element {
        alignment: 2,
        color: [12, 12, 12, 255],
        ..Element::text(
            "genderLabel",
            Rect::new(430.0, 453.0, 157.0, 30.0),
            "rovio_id_gender",
            "Gender",
            19.0,
        )
    },
    // showRegisterView2 calls genderMaleSelectedAction unless the retained female flag is true.
    Element {
        kind: Kind::Button,
        ..Element::image(
            "gender_male_button",
            Rect::new(497.0, 457.0, 23.0, 23.0),
            GENDER_ON_IMAGE,
        )
    },
    Element {
        color: [12, 12, 12, 255],
        ..Element::text(
            "maleGenderLabel",
            Rect::new(471.0, 458.0, 75.0, 21.0),
            "rovio_id_male",
            "Male",
            19.0,
        )
    },
    Element {
        kind: Kind::Button,
        ..Element::image(
            "gender_female_button",
            Rect::new(497.0, 457.0, 23.0, 23.0),
            GENDER_OFF_IMAGE,
        )
    },
    Element {
        color: [12, 12, 12, 255],
        ..Element::text(
            "femaleGenderLabel",
            Rect::new(415.0, 458.0, 187.0, 21.0),
            "rovio_id_female",
            "Female",
            19.0,
        )
    },
];

const REGISTER1: ViewLayout = layout(&[
    BG,
    LOGO,
    Element {
        color: GREY,
        ..Element::text(
            "dobLabel",
            Rect::new(323.0, 329.0, 300.0, 38.0),
            "rovio_id_birthday",
            "Date of Birth",
            19.0,
        )
    },
    date_button(
        "dayTextField",
        Rect::new(312.0, 364.0, 120.0, 37.0),
        "rovio_id_date_field_day",
        "Day",
    ),
    date_button(
        "monthTextField",
        Rect::new(450.0, 364.0, 120.0, 37.0),
        "rovio_id_date_field_month",
        "Month",
    ),
    date_button(
        "yearTextField",
        Rect::new(593.0, 364.0, 120.0, 37.0),
        "rovio_id_date_field_year",
        "Year",
    ),
    Element {
        alignment: 1,
        color: GREY,
        ..Element::text(
            "tosLabel",
            Rect::new(260.0, 421.0, 506.0, 27.0),
            "rovio_id_agree_by_sign_in",
            "By registering, you are agreeing to our",
            21.0,
        )
    },
    TERMS[0],
    TERMS[1],
    TERMS[2],
    Element {
        text_key: Some("rovio_id_continue"),
        fallback: "CONTINUE",
        font_name: "OpenSans-CondensedBold",
        font_size: 30.0,
        ..Element::button(
            "continueButton",
            Rect::new(360.0, 515.0, 304.0, 54.0),
            SUBMIT_IMAGE,
            SUBMIT_DOWN,
        )
    },
    // Misleading outlet name: this is the top-right close/cancel button, not Back.
    Element::button(
        "backButton",
        Rect::new(760.0, 182.0, 51.0, 51.0),
        CLOSE_IMAGE,
        CLOSE_DOWN,
    ),
    Element::button(
        "questionButton",
        Rect::new(219.0, 182.0, 51.0, 51.0),
        "skynestdata/images/identity/button_questionmark.png",
        "skynestdata/images/identity/button_questionmark_down.png",
    ),
    REGISTER1_TITLE[0],
    REGISTER1_TITLE[1],
    // Hidden date pickers are separate DATE_PICKERS metadata; draw a shown picker last.
]);

const REGISTER2: ViewLayout = layout(&[
    BG,
    // Hidden padding outlets are moved to each field's rightView, not root paint.
    LOGO,
    field(
        "emailTextField",
        Rect::new(322.0, 348.0, 376.0, 38.0),
        "rovio_id_email",
        "Email",
        18.0,
    ),
    field(
        "passwordTextField",
        Rect::new(321.0, 396.0, 376.0, 38.0),
        "rovio_id_password",
        "Password",
        18.0,
    ),
    cap(
        "emailTextFieldLeft",
        Rect::new(317.0, 348.0, 12.0, 38.0),
        true,
    ),
    cap(
        "emailTextFieldRight",
        Rect::new(693.0, 348.0, 12.0, 38.0),
        false,
    ),
    cap(
        "passwordTextFieldLeft",
        Rect::new(317.0, 396.0, 12.0, 38.0),
        true,
    ),
    cap(
        "passwordTextFieldRight",
        Rect::new(692.0, 396.0, 12.0, 38.0),
        false,
    ),
    REGISTER2_TITLE[0],
    REGISTER2_TITLE[1],
    GENDER[0],
    GENDER[1],
    GENDER[2],
    GENDER[3],
    GENDER[4],
    Element {
        text_key: Some("rovio_id_sign_up"),
        fallback: "REGISTER",
        font_name: "OpenSans-CondensedBold",
        font_size: 30.0,
        ..Element::button(
            "registerButton",
            Rect::new(360.0, 515.0, 304.0, 54.0),
            SUBMIT_IMAGE,
            SUBMIT_DOWN,
        )
    },
    Element::button(
        "backButton",
        Rect::new(219.0, 182.0, 51.0, 51.0),
        BACK_IMAGE,
        BACK_DOWN,
    ),
    Element::button(
        "closeButton",
        Rect::new(759.0, 182.0, 51.0, 51.0),
        CLOSE_IMAGE,
        CLOSE_DOWN,
    ),
    error_icon("emailErrorButton", Rect::new(672.0, 352.0, 30.0, 30.0)),
    error_icon("passwordErrorButton", Rect::new(672.0, 400.0, 30.0, 30.0)),
    Element {
        kind: Kind::Button,
        ..Element::image(
            "passwordTooltipButton",
            Rect::new(671.0, 400.0, 30.0, 30.0),
            "skynestdata/images/identity/milkshake_questionmark_noti.png",
        )
    },
]);

const THANKS: ViewLayout = layout(&[
    BG,
    LOGO,
    Element::button(
        "okButton",
        Rect::new(760.0, 547.0, 51.0, 51.0),
        OK_IMAGE,
        OK_DOWN,
    ),
    Element {
        color: DARK,
        alignment: 1,
        max_lines: 4,
        ..Element::text(
            "pleaseActivateLabel",
            Rect::new(271.0, 424.0, 482.0, 138.0),
            "rovio_id_sign_up_success_message",
            "Please activate your account by clicking the verification link in the email from Rovio within 48 hours.",
            21.0,
        )
    },
    Element {
        color: [191, 9, 30, 255],
        alignment: 1,
        font_name: "OpenSans-CondensedBold",
        ..Element::text(
            "ThanksForRegisteringLabel",
            Rect::new(251.0, 245.0, 523.0, 89.0),
            "rovio_id_sign_up_success_title",
            "THANKS FOR REGISTERING!",
            38.0,
        )
    },
    Element {
        color: DARK,
        alignment: 1,
        max_lines: 2,
        ..Element::text(
            "emailSentLabel",
            Rect::new(256.0, 308.0, 512.0, 75.0),
            "rovio_id_verification_mail_sent",
            "A verification email has been sent to:",
            21.0,
        )
    },
    Element {
        color: [190, 0, 26, 255],
        alignment: 1,
        max_lines: 2,
        text_key: None,
        // Wrapper replaces the nib's sample address; never display a fake email.
        ..Element::text(
            "registrationEmail",
            Rect::new(260.0, 393.0, 504.0, 30.0),
            "",
            "",
            21.0,
        )
    },
]);

const FAILURE: ViewLayout = layout(&[
    Element {
        rect: Rect::new(212.0, 164.0, 600.0, 450.0),
        ..BG
    },
    Element {
        color: [188, 0, 28, 255],
        alignment: 1,
        font_name: "OpenSans-CondensedBold",
        ..Element::text(
            "mainLabel",
            Rect::new(414.0, 276.0, 197.0, 50.0),
            "rovio_id_message_cannot_create_account_title",
            "SORRY!",
            38.0,
        )
    },
    Element {
        color: DARK,
        alignment: 1,
        max_lines: 2,
        ..Element::text(
            "cannotRegisterLabel",
            Rect::new(284.0, 364.0, 456.0, 50.0),
            "rovio_id_message_cannot_create_account",
            "We could not create your account.",
            21.0,
        )
    },
    Element::button(
        "okButton",
        Rect::new(759.0, 547.0, 51.0, 51.0),
        OK_IMAGE,
        OK_DOWN,
    ),
    Element {
        rect: Rect::new(382.0, 153.0, 260.0, 75.0),
        ..LOGO
    },
]);

pub(crate) fn layout_for(view: AccountView) -> Option<&'static ViewLayout> {
    Some(match view {
        AccountView::Register1 => &REGISTER1,
        AccountView::Register2 => &REGISTER2,
        AccountView::ThanksForRegistering => &THANKS,
        AccountView::RegistrationFailure => &FAILURE,
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct DatePicker {
    pub name: &'static str,
    pub field: &'static str,
    pub rect: Rect,
    pub tag: u8,
}

/// Nib order; initially hidden, one component, clips children, selection indicator.
pub(crate) const DATE_PICKERS: [DatePicker; 3] = [
    DatePicker {
        name: "dayFieldPicker",
        field: "dayTextField",
        rect: Rect::new(308.0, 406.0, 118.0, 162.0),
        tag: 0,
    },
    DatePicker {
        name: "monthFieldPicker",
        field: "monthTextField",
        rect: Rect::new(451.0, 406.0, 118.0, 162.0),
        tag: 1,
    },
    DatePicker {
        name: "yearFieldPicker",
        field: "yearTextField",
        rect: Rect::new(600.0, 406.0, 118.0, 162.0),
        tag: 2,
    },
];
pub(crate) const PICKER_BACKGROUND: [f32; 4] = [1.0, 1.0, 1.0, 0.98];
pub(crate) const PICKER_BORDER: [f32; 4] = [0.5, 0.5, 0.5, 1.0];
pub(crate) const PICKER_CORNER_RADIUS: f32 = 5.0;
pub(crate) const PICKER_BORDER_WIDTH: f32 = 1.0;
pub(crate) const DATE_PLACEHOLDER_PREFIX: &str = " ";
pub(crate) const DATE_VALUE_PREFIX: &str = "   ";
pub(crate) const TERMS_URL: &str = "http://www.rovio.com/eula";
pub(crate) const PRIVACY_URL: &str = "http://www.rovio.com/privacy";
pub(crate) const MONTH_LABELS: [(&str, &str); 12] = [
    ("rovio_id_month_jan", "Jan"),
    ("rovio_id_month_feb", "Feb"),
    ("rovio_id_month_mar", "Mar"),
    ("rovio_id_month_apr", "Apr"),
    ("rovio_id_month_may", "May"),
    ("rovio_id_month_jun", "Jun"),
    ("rovio_id_month_jul", "Jul"),
    ("rovio_id_month_aug", "Aug"),
    ("rovio_id_month_sep", "Sep"),
    ("rovio_id_month_oct", "Oct"),
    ("rovio_id_month_nov", "Nov"),
    ("rovio_id_month_dec", "Dec"),
];

/// 10076B92C: day rows remain 1..=31 even for February. Years end at the current
/// local-calendar year. Calendar validity/age is checked by the controller later.
pub(crate) fn picker_row_count(tag: u8, current_year: i32) -> usize {
    match tag {
        0 => 31,
        1 => 12,
        2 => current_year.saturating_sub(1899).max(0) as usize,
        _ => 0,
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Group {
    pub name: &'static str,
    pub rect: Rect,
    pub spacing: i32,
    pub elements: &'static [Element],
    pub double_height_in_russian: bool,
}

const REGISTER1_GROUPS: [Group; 2] = [
    Group {
        name: "terms",
        rect: Rect::new(260.0, 451.0, 506.0, 26.0),
        spacing: 0,
        elements: &TERMS,
        double_height_in_russian: true,
    },
    Group {
        name: "title",
        rect: Rect::new(260.0, 266.0, 506.0, 39.0),
        spacing: 4,
        elements: &REGISTER1_TITLE,
        double_height_in_russian: false,
    },
];
const REGISTER2_GROUPS: [Group; 2] = [
    Group {
        name: "title",
        rect: Rect::new(260.0, 266.0, 506.0, 39.0),
        spacing: 4,
        elements: &REGISTER2_TITLE,
        double_height_in_russian: false,
    },
    Group {
        name: "gender",
        rect: Rect::new(256.0, 454.0, 506.0, 28.0),
        spacing: 8,
        elements: &GENDER,
        double_height_in_russian: false,
    },
];

pub(crate) fn groups(view: AccountView) -> &'static [Group] {
    match view {
        AccountView::Register1 => &REGISTER1_GROUPS,
        AccountView::Register2 => &REGISTER2_GROUPS,
        _ => &[],
    }
}

/// SHVBox 10076971C/10076999C/100769B24/100769CB8/100769DA0/100769E6C.
/// Pass one measured text width per element; non-label widths are ignored.
/// Returns canvas-space rectangles in group element order. Never paint the raw
/// overlapping gender nib frames without running this original reflow step.
pub(crate) fn group_rects(
    group: &Group,
    measured_widths: &[f64],
    russian_locale: bool,
) -> Vec<Rect> {
    let mut rects: Vec<_> = group
        .elements
        .iter()
        .enumerate()
        .map(|(index, element)| {
            let mut rect = element.rect;
            if element.kind == Kind::Label
                && let Some(width) = measured_widths.get(index)
            {
                rect.width = ((*width + 0.5) as f32).floor();
            }
            rect
        })
        .collect();
    if rects.is_empty() {
        return rects;
    }
    let available = group.rect.width as i32;
    let height = group.rect.height
        * if russian_locale && group.double_height_in_russian {
            2.0
        } else {
            1.0
        };
    let width_sum = |items: &[Rect]| {
        items.iter().fold(0_i32, |sum, item| {
            (f64::from(sum) + f64::from(item.width)) as i32
        })
    };
    let total = width_sum(&rects).wrapping_add(group.spacing.wrapping_mul(rects.len() as i32 - 1));
    if total <= available {
        let mut x = group.rect.x + ((available - total) / 2) as f32;
        for rect in &mut rects {
            rect.x = x;
            rect.y = group.rect.y
                + if rect.height < height {
                    height - rect.height
                } else {
                    0.0
                };
            x += rect.width + group.spacing as f32;
        }
        return rects;
    }
    // Native overflow partition deliberately ignores inter-element spacing.
    let mut count = 0;
    let mut sum = 0_i32;
    for rect in &rects {
        let next = f64::from(sum) + f64::from(rect.width);
        if next > f64::from(available) {
            break;
        }
        sum = next as i32;
        count += 1;
    }
    let mut x = ((available - width_sum(&rects[..count])) / 2) as f32;
    for rect in &mut rects[..count] {
        rect.x = group.rect.x + x;
        rect.y = group.rect.y;
        x += rect.width + group.spacing as f32;
    }
    // A zero-size first row keeps the first child's original x; row two shares
    // row one's left edge, rather than being centered independently.
    x = (rects[0].x - group.rect.x) as i32 as f32;
    let y = group.rect.y + rects[0].height;
    for rect in &mut rects[count..] {
        rect.x = group.rect.x + x;
        rect.y = y;
        x += rect.width + group.spacing as f32;
    }
    rects
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_fields_are_native_controls_and_no_checkbox_is_invented() {
        let view = layout_for(AccountView::Register1).unwrap();
        let dates: Vec<_> = view
            .elements
            .iter()
            .filter(|e| e.name.ends_with("TextField"))
            .collect();
        assert_eq!(dates.len(), 3);
        assert!(dates.iter().all(|e| e.kind == Kind::Button));
        assert_eq!(dates[0].font_name, ".HelveticaNeueInterface-Regular");
        assert_eq!(dates[2].rect, Rect::new(593.0, 364.0, 120.0, 37.0));
        assert!(!view.elements.iter().any(|e| e.name.contains("checkbox")));
        let close = view
            .elements
            .iter()
            .find(|e| e.name == "backButton")
            .unwrap();
        assert_eq!(close.image, Some(CLOSE_IMAGE));
        assert_eq!(close.rect.x, 760.0);
        assert_eq!(DATE_PICKERS[1].tag, 1);
        assert_eq!(DATE_PICKERS[1].field, "monthTextField");
        assert_eq!(DATE_PICKERS[1].rect, Rect::new(451.0, 406.0, 118.0, 162.0));
    }

    #[test]
    fn picker_rows_follow_current_year_not_release_year_or_age_cutoff() {
        assert_eq!(picker_row_count(0, 2026), 31);
        assert_eq!(picker_row_count(1, 2026), 12);
        assert_eq!(picker_row_count(2, 2026), 127);
        assert_eq!(picker_row_count(2, 1900), 1);
        assert_eq!(picker_row_count(2, 1899), 0);
        assert_eq!(MONTH_LABELS[0], ("rovio_id_month_jan", "Jan"));
        assert_eq!(MONTH_LABELS[11], ("rovio_id_month_dec", "Dec"));
    }

    #[test]
    fn gender_reflow_preserves_button_width_and_native_bottom_alignment() {
        let group = &groups(AccountView::Register2)[1];
        let rects = group_rects(group, &[70.2, 999.0, 40.2, 999.0, 64.6], false);
        assert_eq!(
            rects,
            vec![
                Rect::new(382.0, 454.0, 70.0, 30.0),
                Rect::new(460.0, 459.0, 23.0, 23.0),
                Rect::new(491.0, 461.0, 40.0, 21.0),
                Rect::new(539.0, 459.0, 23.0, 23.0),
                Rect::new(570.0, 461.0, 65.0, 21.0),
            ]
        );
        assert_eq!(group.elements[1].image, Some(GENDER_ON_IMAGE));
        assert_eq!(group.elements[3].image, Some(GENDER_OFF_IMAGE));
    }

    #[test]
    fn terms_russian_height_and_two_row_anchor_are_native() {
        let group = &groups(AccountView::Register1)[0];
        let one = group_rects(group, &[100.0, 30.0, 100.0], true);
        assert!(one.iter().all(|r| r.y == 477.0));
        let two = group_rects(group, &[400.0, 30.0, 150.0], false);
        assert_eq!(two[0], Rect::new(298.0, 451.0, 400.0, 26.0));
        assert_eq!(two[1], Rect::new(698.0, 451.0, 30.0, 26.0));
        assert_eq!(two[2], Rect::new(298.0, 477.0, 150.0, 26.0));
        let oversized = group_rects(group, &[600.0, 30.0, 150.0], false);
        assert_eq!(oversized[0], Rect::new(280.0, 477.0, 600.0, 26.0));
    }

    #[test]
    fn result_screens_keep_original_offsets_and_no_fake_account_address() {
        let success = layout_for(AccountView::ThanksForRegistering).unwrap();
        let email = success
            .elements
            .iter()
            .find(|e| e.name == "registrationEmail")
            .unwrap();
        assert_eq!(email.fallback, "");
        assert_eq!(email.text_key, None);
        assert_eq!(email.font_name, "OpenSans");
        let failure = layout_for(AccountView::RegistrationFailure).unwrap();
        assert_eq!(failure.elements[0].rect.y, 164.0);
        assert_eq!(failure.elements.last().unwrap().rect.y, 153.0);
        let register2 = layout_for(AccountView::Register2).unwrap();
        assert_eq!(
            register2.elements.last().unwrap().name,
            "passwordTooltipButton"
        );
        assert!(!register2.elements.last().unwrap().hidden);
        assert!(
            register2
                .elements
                .iter()
                .find(|e| e.name == "emailErrorButton")
                .unwrap()
                .hidden
        );
    }
}
