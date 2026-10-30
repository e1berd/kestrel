use std::time::SystemTime;

use gpui::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, div, rems};
use gpui_component::{Icon, Sizable as _, h_flex};
use gpui_kit_assets::IconName;

use super::Workspace;
use crate::appearance::{self, dp};

impl Workspace {
    pub(super) fn render_status_bar(&self, cx: &mut Context<Self>) -> AnyElement {
        let palette = appearance::palette(cx);
        let mut bar =
            h_flex().flex_none().px(dp(18.)).py(dp(6.)).gap(dp(16.)).items_center().text_size(rems(0.76)).text_color(palette.faint);
        if let Some(view) = self.active_view() {
            let view = view.read(cx);
            let project = view.project.read(cx);
            if let Some(git) = &project.git {
                let changes = match git.changed {
                    0 => "clean".to_string(),
                    1 => "1 changed file".to_string(),
                    n => format!("{n} changed files"),
                };
                bar = bar.child(
                    h_flex().gap(dp(6.)).child(Icon::new(IconName::GitBranch).xsmall()).child(format!("{} · {changes}", git.branch)),
                );
            }
            if let Some(tab) = view.active_tab() {
                let path = tab.read(cx).path.clone();
                let relative = path.strip_prefix(project.root()).unwrap_or(&path).display().to_string();
                bar = bar.child(div().child(relative));
            }
            bar = bar.child(div().flex_1());
            if let Some(synced) = project.last_sync {
                bar = bar.child(
                    h_flex().gap(dp(6.)).child(Icon::new(IconName::RefreshCw).xsmall()).child(format!("OpenAPI synced {}", ago(synced))),
                );
            }
        } else {
            bar = bar.child(div().flex_1());
        }
        if !self.opening.is_empty() {
            bar = bar.child("Opening collections…");
        }
        bar.child(concat!("Kestrel ", env!("CARGO_PKG_VERSION"))).into_any_element()
    }
}

fn ago(moment: SystemTime) -> String {
    let seconds = SystemTime::now().duration_since(moment).map(|d| d.as_secs()).unwrap_or(0);
    match seconds {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", seconds / 60),
        _ => format!("{} h ago", seconds / 3600),
    }
}
