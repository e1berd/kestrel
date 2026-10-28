use std::cell::RefCell;
use std::rc::Rc;

use gpui::{App, AppContext as _, IntoElement as _, ParentElement as _, Styled as _, Window};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputState};
use gpui_component::{WindowExt as _, h_flex, v_flex};

use crate::appearance::dp;
use crate::interact::PastePrompt;
use crate::ui::hint;

const EXPLANATION: &str = "The browser opened the sign-in page. Its callback URL is not on this machine, so once you have signed in, \
copy the address the browser ended on and paste it here. To skip this step, add http://127.0.0.1:<port>/callback to the client's \
redirect URIs and use it as the callback URL: Kestrel then catches the redirect itself.";

pub fn ask_for_redirect(prompt: PastePrompt, window: &mut Window, cx: &mut App) {
    let input = cx.new(|cx| InputState::new(window, cx).placeholder(format!("{}?code=…", prompt.callback)));
    let reply = Rc::new(RefCell::new(Some(prompt.reply)));
    window.open_dialog(cx, move |dialog, _, cx| {
        let input = input.clone();
        let reply = reply.clone();
        dialog
            .title("Finish signing in")
            .w(dp(640.))
            .child(v_flex().gap_3().child(hint(EXPLANATION, cx).text_sm()).child(Input::new(&input)))
            .child(
                h_flex()
                    .justify_end()
                    .pt_2()
                    .child(Button::new("redirect-continue").primary().label("Continue").on_click(move |_, window, cx| {
                        if let Some(reply) = reply.borrow_mut().take() {
                            if reply.send(input.read(cx).value().trim().to_string()).is_err() {
                                volley_core::report::report_error("finish signing in", "the request that asked for it is gone");
                            }
                        }
                        window.close_dialog(cx);
                    }))
                    .into_any_element(),
            )
    });
}
