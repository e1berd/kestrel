use std::rc::Rc;

use gpui::{App, AppContext as _, IntoElement as _, ParentElement as _, SharedString, Styled as _, Window};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputEvent, InputState};
use gpui_component::{WindowExt as _, h_flex};

use crate::appearance::dp;

type Submit = Rc<dyn Fn(String, &mut Window, &mut App)>;

pub fn ask_text(
    window: &mut Window,
    cx: &mut App,
    title: impl Into<SharedString>,
    placeholder: impl Into<SharedString>,
    value: impl Into<SharedString>,
    on_submit: impl Fn(String, &mut Window, &mut App) + 'static,
) {
    let placeholder = placeholder.into();
    let value = value.into();
    let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(value));
    input.update(cx, |input, cx| input.focus(window, cx));
    let on_submit: Submit = Rc::new(on_submit);
    let submit = {
        let input = input.clone();
        move |window: &mut Window, cx: &mut App| {
            let text = input.read(cx).value().trim().to_string();
            if text.is_empty() {
                return;
            }
            window.close_dialog(cx);
            on_submit(text, window, cx);
        }
    };
    let submit = Rc::new(submit);
    let on_enter = submit.clone();
    window
        .subscribe(&input, cx, move |_, event: &InputEvent, window, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                on_enter(window, cx);
            }
        })
        .detach();
    let title: SharedString = title.into();
    window.open_dialog(cx, move |dialog, _, _| {
        let submit = submit.clone();
        dialog.title(title.clone()).w(dp(460.)).child(Input::new(&input)).child(
            h_flex()
                .justify_end()
                .pt_2()
                .child(Button::new("dialog-ok").primary().label("OK").on_click(move |_, window, cx| submit(window, cx)))
                .into_any_element(),
        )
    });
}

pub fn confirm(
    window: &mut Window,
    cx: &mut App,
    title: impl Into<SharedString>,
    description: impl Into<SharedString>,
    ok_text: impl Into<SharedString>,
    on_ok: impl Fn(&mut Window, &mut App) + 'static,
) {
    let title: SharedString = title.into();
    let description: SharedString = description.into();
    let ok_text: SharedString = ok_text.into();
    let on_ok = Rc::new(on_ok);
    window.open_alert_dialog(cx, move |alert, _, _| {
        let on_ok = on_ok.clone();
        alert.title(title.clone()).description(description.clone()).show_cancel(true).ok_text(ok_text.clone()).on_ok(
            move |_, window, cx| {
                on_ok(window, cx);
                true
            },
        )
    });
}
