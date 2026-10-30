use gpui::prelude::FluentBuilder as _;
use gpui::{
    App, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Render, StatefulInteractiveElement as _,
    Styled as _, Window, div, rems,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::Input;
use gpui_component::{Disableable as _, WindowExt as _, h_flex, v_flex};
use kestrel_import::Format;
use kestrel_openapi::GroupBy;

use super::ImportDialog;
use crate::appearance::{self, dp};
use crate::ui::{ReportUnlessGone as _, eyebrow, hint, segmented};

impl ImportDialog {
    fn render_format_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut options = vec![(None, "Detect automatically".to_string())];
        options.extend(Format::ALL.into_iter().map(|format| (Some(format), format.name().into())));
        let weak = cx.entity().downgrade();
        let description = self.format.map(Format::description).unwrap_or("Choose an OpenAPI / Swagger spec or an Apidog project export.");
        v_flex()
            .gap(dp(8.))
            .child(eyebrow("Format", cx))
            .child(segmented(
                "import-format",
                options,
                &self.format,
                move |format, window, cx| {
                    weak.update(cx, |view, cx| {
                        if !view.busy {
                            view.format = format;
                            view.load_source(window, cx);
                        }
                    })
                    .report_unless_gone("choose an import format");
                },
                cx,
            ))
            .child(hint(description, cx))
    }

    fn render_source(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().gap(dp(8.)).child(eyebrow("Source file", cx)).child(
            h_flex().gap(dp(12.)).items_center().child(path_label(self.source.as_deref(), "No file selected", cx)).child(
                Button::new("import-source")
                    .label("Choose file…")
                    .disabled(self.busy)
                    .on_click(cx.listener(|view, _, window, cx| view.choose_source(window, cx))),
            ),
        )
    }

    fn render_preview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = appearance::palette(cx);
        let mut preview = v_flex().gap(dp(8.)).p(dp(14.)).rounded(dp(10.)).bg(palette.surface).border_1().border_color(palette.border_soft);
        if let Some(prepared) = &self.prepared {
            let summary = &prepared.preview;
            preview = preview
                .child(div().font_weight(FontWeight::SEMIBOLD).child(summary.title.clone()))
                .child(hint(&format!("{} · {} requests · {} folders", summary.format.name(), summary.requests, summary.folders), cx))
                .children(
                    summary.warnings.iter().map(|warning| div().text_size(rems(0.8)).text_color(palette.warning).child(warning.clone())),
                );
        } else {
            preview = preview.child(hint(
                if self.busy { "Reading the import file…" } else { "Select a file to preview the collection before importing." },
                cx,
            ));
        }
        preview
    }

    fn render_grouping(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.entity().downgrade();
        v_flex().gap(dp(8.)).child(eyebrow("Organize OpenAPI requests", cx)).child(segmented(
            "import-grouping",
            vec![(GroupBy::Tags, "By tags".into()), (GroupBy::Path, "By URL path".into())],
            &self.group_by,
            move |group_by, window, cx| {
                weak.update(cx, |view, cx| {
                    if !view.busy {
                        view.group_by = group_by;
                        view.load_source(window, cx);
                    }
                })
                .report_unless_gone("choose import grouping");
            },
            cx,
        ))
    }

    fn render_destination(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap(dp(8.))
            .child(eyebrow("Collection", cx))
            .child(Input::new(&self.name).disabled(self.busy))
            .child(
                h_flex()
                    .gap(dp(12.))
                    .items_center()
                    .child(path_label(self.destination.as_deref(), "Choose an empty destination folder", cx))
                    .child(
                        Button::new("import-destination")
                            .label("Choose folder…")
                            .disabled(self.busy)
                            .on_click(cx.listener(|view, _, window, cx| view.choose_destination(window, cx))),
                    ),
            )
            .child(hint("The collection will be saved as Bruno-compatible files in this folder.", cx))
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .gap(dp(8.))
            .justify_end()
            .child(Button::new("import-cancel").label("Cancel").on_click(|_, window, cx| window.close_dialog(cx)))
            .child(
                Button::new("import-save")
                    .primary()
                    .label("Import collection")
                    .loading(self.busy)
                    .disabled(self.busy || self.prepared.is_none() || self.destination.is_none())
                    .on_click(cx.listener(|view, _, window, cx| view.import(window, cx))),
            )
    }

    fn render_open_option(&self, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap(dp(6.))
            .child(
                Checkbox::new("import-open-after")
                    .label("Open collection after import")
                    .checked(self.open_after_import)
                    .disabled(self.busy)
                    .on_click(cx.listener(|view, checked: &bool, _, cx| {
                        view.open_after_import = *checked;
                        cx.notify();
                    })),
            )
            .when(self.prepared.as_ref().is_some_and(|prepared| !prepared.preview.open_after_import_by_default()), |element| {
                element.child(hint("Large collections stay on disk by default. Opening them uses additional memory.", cx))
            })
    }
}

impl Render for ImportDialog {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let is_openapi =
            self.prepared.as_ref().map(|prepared| prepared.preview.format) == Some(Format::OpenApi) || self.format == Some(Format::OpenApi);
        v_flex()
            .gap(dp(18.))
            .when(self.busy, |element| element.child(crate::transfer_progress::render(&self.progress, cx)))
            .child(
                v_flex()
                    .id("import-content")
                    .max_h(window.viewport_size().height * 0.65)
                    .overflow_y_scroll()
                    .gap(dp(18.))
                    .child(self.render_format_picker(cx))
                    .child(self.render_source(cx))
                    .when(is_openapi, |element| element.child(self.render_grouping(cx)))
                    .child(self.render_preview(cx))
                    .child(self.render_destination(cx))
                    .when(self.prepared.is_some(), |element| element.child(self.render_open_option(cx)))
                    .children(self.error.as_ref().map(|error| crate::ui::error_message("import-copy-error", error, cx))),
            )
            .child(self.render_footer(cx))
    }
}

fn path_label(path: Option<&std::path::Path>, placeholder: &str, cx: &App) -> impl IntoElement {
    div()
        .flex_1()
        .min_w_0()
        .text_size(rems(0.85))
        .text_color(appearance::palette(cx).muted)
        .child(path.map(|path| path.display().to_string()).unwrap_or_else(|| placeholder.into()))
}
