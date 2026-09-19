use super::*;

#[test]
fn real_registration_artwork_picker_input_and_required_fields_do_not_reach_game() {
    let Some(sandbox) = ShippedDataSandbox::new() else {
        return;
    };
    let mut app = sandbox.app();
    app.runtime
        .execute_source("_G.SkynestAccount.native_login(true, false, true)")
        .unwrap();
    app.synchronize_account_ui().unwrap();
    let root = unique_directory("stella-registration-visual-qa");
    let initial = paint(&mut app, 0.0);
    artifact_with_backdrop(&initial)
        .save(root.join("register-birthday.png"))
        .unwrap();
    assert_eq!(app.account_painter.hit(370.0, 380.0), Some("dayTextField"));
    click(&mut app, 512.0, 540.0);
    assert_eq!(
        app.runtime.account_ui().unwrap().view,
        AccountView::Register1
    );
    let missing = paint(&mut app, 0.0);
    assert!(
        initial != missing,
        "missing birthday fields paint red native backgrounds"
    );
    artifact_with_backdrop(&missing)
        .save(root.join("register-birthday-required.png"))
        .unwrap();
    for x in [370.0, 510.0, 650.0] {
        click(&mut app, x, 380.0);
        assert_eq!(
            app.account_ui.focused(),
            None,
            "date picker is not an IME text field"
        );
        let opened = paint(&mut app, 0.0);
        if x == 650.0 {
            artifact_with_backdrop(&opened)
                .save(root.join("register-year-picker.png"))
                .unwrap();
            app.account_ui.scroll_picker(-26);
            let _ = paint(&mut app, 0.0);
        }
        // Second tap on the date field commits the current row then closes.
        click(&mut app, x, 380.0);
        let _ = paint(&mut app, 0.0);
    }
    click(&mut app, 512.0, 540.0);
    assert_eq!(
        app.runtime.account_ui().unwrap().view,
        AccountView::Register2
    );
    let details = paint(&mut app, 0.0);
    artifact_with_backdrop(&details)
        .save(root.join("register-account.png"))
        .unwrap();
    assert_eq!(
        app.account_painter.hit(440.0, 366.0),
        Some("emailTextField")
    );
    assert_eq!(
        app.account_painter.hit(440.0, 414.0),
        Some("passwordTextField")
    );
    click(&mut app, 687.0, 414.0);
    let help = paint(&mut app, 0.0);
    artifact_with_backdrop(&help)
        .save(root.join("register-password-help.png"))
        .unwrap();
    assert!(details != help);
    // Hide tooltip before typing, because its popup correctly occludes input.
    click(&mut app, 900.0, 650.0);
    let _ = paint(&mut app, 0.0);
    click(&mut app, 440.0, 366.0);
    ime(&mut app, Ime::Commit("qa@example.invalid".to_owned()));
    click(&mut app, 440.0, 414.0);
    ime(&mut app, Ime::Commit("abc".to_owned()));
    let _ = paint(&mut app, 0.0);
    click(&mut app, 512.0, 540.0);
    assert!(!app.runtime.account_ui().unwrap().busy);
    let short = paint(&mut app, 0.0);
    artifact_with_backdrop(&short)
        .save(root.join("register-password-short.png"))
        .unwrap();
    // Held/touch Lua globals are published at the frame boundary, not by the
    // host event callback; advance that boundary before inspecting them.
    app.runtime.update(0.0).unwrap();
    assert_game_input_clear(&app);
    let id = app.account_owner.unwrap();
    app.runtime
        .account_ui_action(id, stella_script::AccountUiAction::Cancel)
        .unwrap();
    app.synchronize_account_ui().unwrap();
    assert!(!app.account_ui.visible());
    assert!(app.runtime.audio_output_state().started);
    // Presentation-only snapshots cover both terminal native nibs without
    // pretending the offline runtime accepted an account or issuing requests.
    for (view, filename) in [
        (AccountView::ThanksForRegistering, "register-thanks.png"),
        (AccountView::RegistrationFailure, "register-failure.png"),
    ] {
        let mut state = crate::account_ui::AccountUi::default();
        let snapshot = |view| stella_script::AccountUiSnapshot {
            id: 9000,
            view,
            busy: false,
            field_error: None,
        };
        state.sync(Some(snapshot(AccountView::Register2)));
        state.focus(Some(crate::account_ui::Field::Email));
        state.text("qa@example.invalid");
        state.sync(Some(snapshot(view)));
        let mut painter =
            crate::account_ui::AccountPainter::new(sandbox.data.clone(), &app.runtime);
        let image = painter
            .paint(&app.runtime, &state, 1024, 768, 0.0)
            .unwrap()
            .unwrap();
        artifact_with_backdrop(&image)
            .save(root.join(filename))
            .unwrap();
        assert!(app.runtime.account_ui().is_none());
    }
    eprintln!("registration UI artifacts: {}", root.display());
}
