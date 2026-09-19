//! Native Skynest UIKit layout data, not game-Lua sprite coordinates.
//!
//! Frames/order come from original `*~ipad.nib/objects.nib`; font/label changes
//! come from each view's awakeFromNib. See docs/native-account-layout.md.

use stella_script::AccountView;

pub(crate) mod registration;

pub(crate) use crate::platform_ui_drawing::Rect;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    Image,
    Label,
    Button,
    Field,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Element {
    /// Original outlet name, or an explicit name for an unconnected image.
    pub name: &'static str,
    pub rect: Rect,
    pub kind: Kind,
    /// Bundle-relative logical image name. Resolve UIKit @2x/~ipad variants.
    pub image: Option<&'static str>,
    /// Retained UIKit button background when native installs a foreground image.
    pub button_background: Option<&'static str>,
    pub pressed_image: Option<&'static str>,
    pub error_image: Option<&'static str>,
    pub text_key: Option<&'static str>,
    pub fallback: &'static str,
    pub font_name: &'static str,
    pub font_size: f32,
    pub color: [u8; 4],
    /// NSTextAlignment: 0 left, 1 center, 2 right.
    pub alignment: u8,
    pub max_lines: u32,
    /// UIImageView contentMode; 1 is aspect-fit, 0 is scale-to-fill.
    pub content_mode: u8,
    pub hidden: bool,
}

impl Element {
    const fn image(name: &'static str, rect: Rect, image: &'static str) -> Self {
        Self {
            name,
            rect,
            kind: Kind::Image,
            image: Some(image),
            button_background: None,
            pressed_image: None,
            error_image: None,
            text_key: None,
            fallback: "",
            font_name: "OpenSans",
            font_size: 0.0,
            color: [255; 4],
            alignment: 0,
            max_lines: 1,
            content_mode: 0,
            hidden: false,
        }
    }

    const fn button(
        name: &'static str,
        rect: Rect,
        image: &'static str,
        pressed: &'static str,
    ) -> Self {
        Self {
            kind: Kind::Button,
            pressed_image: Some(pressed),
            alignment: 1,
            ..Self::image(name, rect, image)
        }
    }

    const fn text(
        name: &'static str,
        rect: Rect,
        key: &'static str,
        fallback: &'static str,
        size: f32,
    ) -> Self {
        Self {
            kind: Kind::Label,
            image: None,
            text_key: Some(key),
            fallback,
            font_size: size,
            color: [0, 0, 0, 255],
            ..Self::image(name, rect, "")
        }
    }
}

#[derive(Debug)]
pub(crate) struct ViewLayout {
    pub size: [f32; 2],
    /// Recovered wrapper background is a separate full-window black 50% view.
    pub backdrop: [f32; 4],
    /// Paint in this order, with dynamic error popups appended last.
    pub elements: &'static [Element],
}

const RED: [u8; 4] = [175, 10, 26, 255];
const GREY: [u8; 4] = [75, 74, 76, 255];
const DARK: [u8; 4] = [14, 6, 17, 255];
const BG: Element = Element::image(
    "background",
    Rect::new(212.0, 163.0, 600.0, 450.0),
    "skynestdata/images/identity/milkshake_bg.png",
);
const LOGO: Element = Element::image(
    "logo",
    Rect::new(382.0, 154.0, 260.0, 75.0),
    "skynestdata/images/identity/milkshake_rovio_logo.png",
);
const FIELD_IMAGE: &str = "skynestdata/images/identity/milkshake_textbox.png";
const FIELD_ERROR: &str = "skynestdata/images/identity/milkshake_textbox_error.png";
const LEFT_IMAGE: &str = "skynestdata/images/identity/milkshake_textbox_left.png";
const LEFT_ERROR: &str = "skynestdata/images/identity/milkshake_textbox_error_left.png";
const RIGHT_IMAGE: &str = "skynestdata/images/identity/milkshake_textbox_right.png";
const RIGHT_ERROR: &str = "skynestdata/images/identity/milkshake_textbox_error_right.png";
const ERROR_ICON: &str = "skynestdata/images/identity/milkshake_exclamation_mark_noti.png";
const SUBMIT_IMAGE: &str = "skynestdata/images/identity/milkshake_button_register.png";
const SUBMIT_DOWN: &str = "skynestdata/images/identity/milkshake_button_register_down.png";
const BACK_IMAGE: &str = "skynestdata/images/identity/button_arrow_back.png";
const BACK_DOWN: &str = "skynestdata/images/identity/button_arrow_back_down.png";
const OK_IMAGE: &str = "skynestdata/images/identity/button_ok.png";
const OK_DOWN: &str = "skynestdata/images/identity/button_ok_down.png";

const fn field(
    name: &'static str,
    rect: Rect,
    key: &'static str,
    fallback: &'static str,
    size: f32,
) -> Element {
    Element {
        kind: Kind::Field,
        image: Some(FIELD_IMAGE),
        error_image: Some(FIELD_ERROR),
        ..Element::text(name, rect, key, fallback, size)
    }
}

const fn cap(name: &'static str, rect: Rect, left: bool) -> Element {
    Element {
        error_image: Some(if left { LEFT_ERROR } else { RIGHT_ERROR }),
        ..Element::image(name, rect, if left { LEFT_IMAGE } else { RIGHT_IMAGE })
    }
}

const fn error_icon(name: &'static str, rect: Rect) -> Element {
    Element {
        kind: Kind::Button,
        hidden: true,
        ..Element::image(name, rect, ERROR_ICON)
    }
}

const fn layout(elements: &'static [Element]) -> ViewLayout {
    ViewLayout {
        size: [1024.0, 768.0],
        backdrop: [0.0, 0.0, 0.0, 0.5],
        elements,
    }
}

const SIGN_IN: ViewLayout = layout(&[
    BG,
    Element {
        content_mode: 1,
        ..LOGO
    },
    // Hidden padding UIButtons are reparented as UITextField.rightView by awakeFromNib.
    // They reserve 30 points on the right; they do not paint in the root view.
    field(
        "emailTextField",
        Rect::new(321.0, 276.0, 376.0, 38.0),
        "rovio_id_email",
        "Email",
        18.0,
    ),
    field(
        "passwordTextField",
        Rect::new(323.0, 328.0, 376.0, 38.0),
        "rovio_id_password",
        "Password",
        18.0,
    ),
    // awakeFromNib changes only this label's width to its measured text width.
    Element {
        color: RED,
        ..Element::text(
            "forgotPasswordLabel",
            Rect::new(322.0, 367.0, 446.0, 45.0),
            "rovio_id_forgot_password",
            "Forgot your password?",
            19.0,
        )
    },
    Element {
        text_key: Some("rovio_id_sign_in"),
        fallback: "SIGN IN",
        font_name: "OpenSans-CondensedBold",
        font_size: 28.0,
        ..Element::button(
            "signInButton",
            Rect::new(361.0, 463.0, 302.0, 54.0),
            SUBMIT_IMAGE,
            SUBMIT_DOWN,
        )
    },
    // Original SHVBox bounds: (257,525,511,42). It reflows these localized labels.
    Element {
        alignment: 2,
        ..Element::text(
            "dontHaveAccountLabel",
            Rect::new(331.0, 525.0, 225.0, 42.0),
            "rovio_id_not_have_account_yet",
            "Don't have an account yet?",
            19.0,
        )
    },
    Element {
        color: RED,
        ..Element::text(
            "registerLabel",
            Rect::new(564.0, 525.0, 130.0, 42.0),
            "rovio_id_sign_up_now",
            " Register now!",
            19.0,
        )
    },
    Element::button(
        "closeButton",
        Rect::new(760.0, 181.0, 51.0, 51.0),
        "skynestdata/images/identity/button_close_topright.png",
        "skynestdata/images/identity/button_close_topright_down.png",
    ),
    Element::button(
        "questionButton",
        Rect::new(218.0, 181.0, 51.0, 51.0),
        "skynestdata/images/identity/button_questionmark.png",
        "skynestdata/images/identity/button_questionmark_down.png",
    ),
    cap(
        "emailTextFieldleftbox",
        Rect::new(316.0, 276.0, 12.0, 38.0),
        true,
    ),
    cap(
        "emailTextFieldrightbox",
        Rect::new(692.0, 276.0, 12.0, 38.0),
        false,
    ),
    cap(
        "passwordTextFieldleftbox",
        Rect::new(316.0, 328.0, 12.0, 38.0),
        true,
    ),
    cap(
        "passwordTextFieldrightbox",
        Rect::new(691.0, 328.0, 12.0, 38.0),
        false,
    ),
    error_icon("emailErrorButton", Rect::new(670.0, 280.0, 30.0, 30.0)),
    error_icon("passwordErrorButton", Rect::new(669.0, 332.0, 30.0, 30.0)),
]);

const FORGOT_PASSWORD: ViewLayout = layout(&[
    BG,
    Element {
        content_mode: 1,
        ..LOGO
    },
    Element {
        color: RED,
        alignment: 1,
        font_name: "OpenSans-CondensedBold",
        ..Element::text(
            "mainLabel",
            Rect::new(240.0, 245.0, 545.0, 87.0),
            "rovio_id_request_new_password",
            "REQUEST NEW PASSWORD",
            38.0,
        )
    },
    Element {
        color: GREY,
        alignment: 1,
        ..Element::text(
            "enterEmailAddressLabel",
            Rect::new(260.0, 306.0, 505.0, 67.0),
            "rovio_id_forgot_password_message",
            "Enter the email address associated with your account.",
            21.0,
        )
    },
    field(
        "emailTextField",
        Rect::new(325.0, 377.0, 376.0, 38.0),
        "rovio_id_email",
        "Email",
        16.0,
    ),
    Element::button(
        "backButton",
        Rect::new(219.0, 182.0, 51.0, 51.0),
        BACK_IMAGE,
        BACK_DOWN,
    ),
    cap(
        "emailTextFieldLeft",
        Rect::new(318.0, 377.0, 12.0, 38.0),
        true,
    ),
    cap(
        "emailTextFieldRight",
        Rect::new(694.0, 377.0, 12.0, 38.0),
        false,
    ),
    error_icon("emailErrorButton", Rect::new(673.0, 381.0, 30.0, 30.0)),
    Element {
        text_key: Some("rovio_id_send_request"),
        fallback: "SEND REQUEST",
        font_name: "OpenSans-CondensedBold",
        font_size: 28.0,
        ..Element::button(
            "sendRequestButton",
            Rect::new(361.0, 463.0, 302.0, 54.0),
            SUBMIT_IMAGE,
            SUBMIT_DOWN,
        )
    },
]);

const fn help(
    image: &'static str,
    key: &'static str,
    fallback: &'static str,
    final_page: bool,
) -> [Element; 5] {
    [
        BG,
        Element {
            content_mode: 1,
            ..Element::image("imageView", Rect::new(289.0, 271.0, 451.0, 204.0), image)
        },
        Element {
            button_background: if final_page {
                Some("skynestdata/images/identity/button_arrow_forward_bottom_right.png")
            } else {
                None
            },
            ..Element::button(
                "nextButton",
                Rect::new(757.0, 545.0, 51.0, 51.0),
                if final_page {
                    OK_IMAGE
                } else {
                    "skynestdata/images/identity/button_arrow_forward_bottom_right.png"
                },
                if final_page {
                    OK_DOWN
                } else {
                    "skynestdata/images/identity/button_arrow_forward_bottom_right_down.png"
                },
            )
        },
        Element {
            color: GREY,
            alignment: 1,
            max_lines: 3,
            ..Element::text(
                "label",
                Rect::new(290.0, 493.0, 448.0, 86.0),
                key,
                fallback,
                19.0,
            )
        },
        LOGO,
    ]
}

const HELP1: ViewLayout = layout(&help(
    "skynestdata/images/identity/save_online.png",
    "rovio_id_crimson_help1",
    "Save scores, stars and achievements online!",
    false,
));
const HELP2: ViewLayout = layout(&help(
    "skynestdata/images/identity/ab_help_sync.png",
    "rovio_id_crimson_help4",
    "Continue playing on another device!",
    false,
));
const HELP3: ViewLayout = layout(&help(
    "skynestdata/images/identity/powerup_help.png",
    "rovio_id_crimson_help_ios",
    "Use the same characters and credits on your iPhone, iPod touch and iPad!",
    true,
));

const NETWORK_FAILURE: ViewLayout = layout(&[
    BG,
    Element::button(
        "backButton",
        Rect::new(218.0, 181.0, 51.0, 51.0),
        BACK_IMAGE,
        BACK_DOWN,
    ),
    LOGO,
    Element::image(
        "globe",
        Rect::new(612.0, 333.0, 125.0, 150.0),
        "skynestdata/images/identity/connect_globe.png",
    ),
    Element::image(
        "phone",
        Rect::new(311.0, 333.0, 100.0, 150.0),
        "skynestdata/images/identity/connect_phone.png",
    ),
    Element::image(
        "notConnected",
        Rect::new(473.0, 362.0, 78.0, 66.0),
        "skynestdata/images/identity/not_connected.png",
    ),
]);

const NOT_VERIFIED: ViewLayout = layout(&[
    BG,
    LOGO,
    Element::button(
        "okButton",
        Rect::new(758.0, 545.0, 51.0, 51.0),
        OK_IMAGE,
        OK_DOWN,
    ),
    Element {
        color: DARK,
        alignment: 1,
        max_lines: 3,
        ..Element::text(
            "PleaseActivateLabel",
            Rect::new(264.0, 436.0, 496.0, 139.0),
            "rovio_id_account_not_confirmed_message",
            "Please activate your account by clicking the verification link in the email from Rovio.",
            21.0,
        )
    },
    Element {
        color: [191, 9, 30, 255],
        alignment: 1,
        max_lines: 2,
        font_name: "OpenSans-CondensedBold",
        ..Element::text(
            "mainLabel",
            Rect::new(243.0, 238.0, 539.0, 122.0),
            "rovio_id_account_not_confirmed_title",
            "YOU HAVE NOT VERIFIED YOUR ACCOUNT YET!",
            38.0,
        )
    },
    Element {
        color: DARK,
        alignment: 1,
        max_lines: 2,
        ..Element::text(
            "verificationEmailSent",
            Rect::new(243.0, 349.0, 537.0, 74.0),
            "rovio_id_verification_mail_resent",
            "A verification email has been re-sent to:",
            21.0,
        )
    },
    // Native wrapper fills this from retained email, never from a placeholder identity.
    Element {
        color: DARK,
        alignment: 1,
        text_key: None,
        ..Element::text(
            "verificationEmail",
            Rect::new(236.0, 425.0, 544.0, 37.0),
            "",
            "",
            21.0,
        )
    },
]);

const RESET_SENT: ViewLayout = layout(&[
    BG,
    LOGO,
    Element {
        color: [248, 0, 9, 255],
        alignment: 1,
        font_name: "OpenSans-CondensedBold",
        ..Element::text(
            "mainLabel",
            Rect::new(245.0, 259.0, 535.0, 50.0),
            "rovio_id_reset_password_sent_title",
            "EMAIL HAS BEEN SENT!",
            38.0,
        )
    },
    Element {
        color: DARK,
        alignment: 1,
        max_lines: 4,
        ..Element::text(
            "infoLabel",
            Rect::new(268.0, 366.0, 471.0, 148.0),
            "rovio_id_reset_password_message",
            "Please check your email for password reset information.",
            21.0,
        )
    },
    Element::button(
        "okButton",
        Rect::new(758.0, 546.0, 51.0, 51.0),
        OK_IMAGE,
        OK_DOWN,
    ),
]);

const PROGRESS: ViewLayout = layout(&[
    BG,
    LOGO,
    Element {
        content_mode: 1,
        ..Element::image(
            "phone",
            Rect::new(316.0, 323.0, 89.0, 158.0),
            "skynestdata/images/identity/connect_phone.png",
        )
    },
    Element::image(
        "progressImageView",
        Rect::new(446.0, 334.0, 95.0, 108.0),
        "skynestdata/images/identity/dot-1.png",
    ),
    Element {
        content_mode: 1,
        ..Element::image(
            "globe",
            Rect::new(587.0, 323.0, 122.0, 158.0),
            "skynestdata/images/identity/connect_globe.png",
        )
    },
]);

pub(crate) fn layout_for(view: AccountView) -> Option<&'static ViewLayout> {
    Some(match view {
        AccountView::SignIn => &SIGN_IN,
        AccountView::ForgotPassword => &FORGOT_PASSWORD,
        AccountView::Help1 => &HELP1,
        AccountView::Help2 => &HELP2,
        AccountView::Help3 => &HELP3,
        AccountView::NoNetworkConnectivity => &NETWORK_FAILURE,
        AccountView::AccountNotVerified => &NOT_VERIFIED,
        AccountView::PasswordResetEmailSent => &RESET_SENT,
        _ => return registration::layout_for(view),
    })
}

pub(crate) fn progress_layout() -> &'static ViewLayout {
    &PROGRESS
}

/// 10076A9FC: dot-1 through dot-6, UIImageView duration=(double)0.6f.
pub(crate) const PROGRESS_IMAGES: [&str; 6] = [
    "skynestdata/images/identity/dot-1.png",
    "skynestdata/images/identity/dot-2.png",
    "skynestdata/images/identity/dot-3.png",
    "skynestdata/images/identity/dot-4.png",
    "skynestdata/images/identity/dot-5.png",
    "skynestdata/images/identity/dot-6.png",
];
pub(crate) const PROGRESS_DURATION: f64 = 0.6_f32 as f64;
pub(crate) const FIELD_RIGHT_PADDING: f32 = 30.0;
pub(crate) const ERROR_POPUP_IMAGE: &str = "skynestdata/images/identity/error_popup_one_row.png";
pub(crate) const ERROR_POPUP_CAPS: [u32; 2] = [60, 30];
pub(crate) const ERROR_POPUP_TEXT_CONSTRAINT: [f32; 2] = [330.0, 135.0];
pub(crate) const ERROR_POPUP_FONT_SIZE: f32 = 18.0;
pub(crate) const ERROR_POPUP_TEXT_TOP_INSET: f32 = 15.0;

/// 10076971C/10076999C/100769CB8/100769DA0: the two SignIn SHVBox links.
/// Measure localized OpenSans19 text first. Native rounds widths through f32
/// floor(width + 0.5), retains the 42-point label heights, and uses zero spacing.
pub(crate) fn sign_in_link_rects(measured_widths: [f64; 2]) -> [Rect; 2] {
    let widths = measured_widths.map(|width| ((width + 0.5) as f32).floor());
    let first_width = widths[0] as i32;
    let total = (f64::from(first_width) + f64::from(widths[1])) as i32;
    if total <= 511 {
        let x = 257.0 + ((511 - total) / 2) as f32;
        return [
            Rect::new(x, 525.0, widths[0], 42.0),
            Rect::new(x + widths[0], 525.0, widths[1], 42.0),
        ];
    }
    if widths[0] <= 511.0 {
        let x = 257.0 + ((511 - first_width) / 2) as f32;
        [
            Rect::new(x, 525.0, widths[0], 42.0),
            Rect::new(x, 567.0, widths[1], 42.0),
        ]
    } else {
        // Native first-row count=0 keeps the first label's original local x=74.
        [
            Rect::new(331.0, 567.0, widths[0], 42.0),
            Rect::new(331.0 + widths[0], 567.0, widths[1], 42.0),
        ]
    }
}

/// 1007689D8 + 100768B3C. `text_size` is measured OpenSans18, constrained
/// to 330x135, word-wrapped; SLabel is white, centered, with two lines.
pub(crate) fn error_popup_rect(anchor: Rect, text_size: [f32; 2]) -> Rect {
    let width = (f64::from(text_size[0]) + 30.0).max(312.0);
    let height = f64::from(text_size[1]) + 30.0;
    let x = ((f64::from(anchor.x) + f64::from(anchor.width) + 12.0 - width) as i32).max(0) as f32;
    let y = ((f64::from(anchor.y) + f64::from(anchor.height) - 2.0) as i32).max(0) as f32;
    Rect::new(x, y, width.ceil() as f32, height.ceil() as f32)
}

/// Desktop content layout for the original bordered UITextFields. The nib
/// paints disabled cap fields above the editable field; do not reverse that
/// order or let our plain raster editor draw text underneath a cap. UIKit's
/// precise internal bezel inset is OS-owned, not recovered from Purple. This
/// cap-derived visible inset is an explicit desktop adaptation.
pub(crate) fn field_content_rect(view: AccountView, field: &Element) -> Rect {
    let left = layout_for(view)
        .into_iter()
        .flat_map(|layout| layout.elements)
        .filter(|cap| cap.name.starts_with(field.name) && cap.image == Some(LEFT_IMAGE))
        .map(|cap| cap.rect.x + cap.rect.width)
        .fold(field.rect.x + 2.0, f32::max);
    let right = field.rect.x + field.rect.width - FIELD_RIGHT_PADDING;
    Rect::new(
        left,
        field.rect.y,
        (right - left).max(0.0),
        field.rect.height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_account_layout_fields_and_painter_order() {
        let sign_in = layout_for(AccountView::SignIn).unwrap();
        assert_eq!(sign_in.size, [1024.0, 768.0]);
        assert_eq!(sign_in.backdrop, [0.0, 0.0, 0.0, 0.5]);
        assert_eq!(sign_in.elements[0].name, "background");
        let email = sign_in
            .elements
            .iter()
            .find(|element| element.name == "emailTextField")
            .unwrap();
        assert_eq!(email.rect, Rect::new(321.0, 276.0, 376.0, 38.0));
        assert_eq!(email.font_name, "OpenSans");
        assert_eq!(email.font_size, 18.0);
        assert_eq!(sign_in.elements.last().unwrap().name, "passwordErrorButton");
        assert!(sign_in.elements.last().unwrap().hidden);
        assert!(layout_for(AccountView::Register1).is_some());
        let help3 = layout_for(AccountView::Help3).unwrap();
        let next = help3
            .elements
            .iter()
            .find(|element| element.name == "nextButton")
            .unwrap();
        assert_eq!(next.image, Some(OK_IMAGE));
        assert_eq!(
            next.button_background,
            Some("skynestdata/images/identity/button_arrow_forward_bottom_right.png")
        );
    }

    #[test]
    fn native_account_error_popup_anchor_and_progress_cycle() {
        assert_eq!(
            error_popup_rect(Rect::new(670.0, 280.0, 30.0, 30.0), [150.0, 25.25]),
            Rect::new(400.0, 308.0, 312.0, 56.0)
        );
        assert_eq!(
            error_popup_rect(Rect::new(1.0, 0.0, 30.0, 30.0), [329.75, 50.0]),
            Rect::new(0.0, 28.0, 360.0, 80.0)
        );
        assert_eq!(PROGRESS_IMAGES.len(), 6);
        assert_eq!(PROGRESS_DURATION, f64::from(0.6_f32));
    }

    #[test]
    fn native_account_link_group_rounding_and_two_row_layout() {
        assert_eq!(
            sign_in_link_rects([225.49, 130.5]),
            [
                Rect::new(334.0, 525.0, 225.0, 42.0),
                Rect::new(559.0, 525.0, 131.0, 42.0),
            ]
        );
        assert_eq!(
            sign_in_link_rects([400.0, 150.0]),
            [
                Rect::new(312.0, 525.0, 400.0, 42.0),
                Rect::new(312.0, 567.0, 150.0, 42.0),
            ]
        );
        assert_eq!(
            sign_in_link_rects([600.0, 150.0]),
            [
                Rect::new(331.0, 567.0, 600.0, 42.0),
                Rect::new(931.0, 567.0, 150.0, 42.0),
            ]
        );
    }
}
