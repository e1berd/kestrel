use gpui::{AppContext as _, Context, Window};
use gpui_component::WindowExt as _;
use gpui_component::notification::Notification;
use kestrel_core::Collection;
use kestrel_core::report::ReportExt as _;
use kestrel_openapi::{GroupBy, SyncOptions};
use serde_yaml_ng::{Mapping, Value};

use super::ProjectView;
use crate::dialogs::ask_text;
use crate::ui::ReportUnlessGone as _;

impl ProjectView {
    pub fn sync_openapi(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let collection = self.project.read(cx).collection.clone();
        let Some((source, group_by)) = collection.doc.data.openapi_sources().into_iter().next() else {
            self.ask_for_spec(window, cx);
            return;
        };
        window.push_notification(Notification::info(format!("Syncing from {source}…")), cx);
        let task = cx.background_spawn(async move {
            let mut collection = collection;
            let spec = kestrel_openapi::load(&source, &collection.root)?;
            kestrel_openapi::sync(
                &mut collection,
                &spec,
                &SyncOptions { group_by: GroupBy::parse(&group_by), prune: false, dry_run: false },
            )
        });
        let project = self.project.clone();
        cx.spawn_in(window, async move |_, cx| {
            let result = task.await;
            project
                .update_in(cx, |project, window, cx| {
                    match result {
                        Ok(report) => {
                            project.last_sync = Some(std::time::SystemTime::now());
                            window.push_notification(Notification::success(sync_summary(&report)), cx)
                        }
                        Err(error) => window.push_notification(Notification::error(format!("Sync failed: {error:#}")), cx),
                    }
                    project.reload(cx);
                })
                .report_unless_gone("finish an OpenAPI sync");
        })
        .detach();
    }

    fn ask_for_spec(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let view = cx.entity().downgrade();
        let placeholder = "../backend/openapi.json or https://…/openapi.json";
        ask_text(window, cx, "Where is the OpenAPI spec?", placeholder, "", move |source, window, cx| {
            view.update(cx, |view, cx| {
                view.project.update(cx, |project, _| project.edit_collection(|collection| remember_spec(collection, &source)));
                view.sync_openapi(window, cx);
            })
            .report_unless_gone("start an OpenAPI sync");
        });
    }
}

fn sync_summary(report: &kestrel_openapi::Report) -> String {
    let mut text = format!("Synced: {report}");
    if !report.stale.is_empty() {
        text.push_str(&format!(". Not in the spec any more: {}", report.stale.join(", ")));
    }
    text
}

fn remember_spec(collection: &mut Collection, source: &str) {
    let extensions = collection.doc.data.extra.entry("extensions".into()).or_insert_with(|| Value::Mapping(Mapping::new()));
    let Some(extensions) = extensions.as_mapping_mut() else { return };
    let tool_extensions = extensions.entry("volley".into()).or_insert_with(|| Value::Mapping(Mapping::new()));
    let Some(tool_extensions) = tool_extensions.as_mapping_mut() else { return };
    let mut entry = Mapping::new();
    entry.insert("sourceUrl".into(), source.into());
    entry.insert("groupBy".into(), "tags".into());
    tool_extensions.insert("openapi".into(), Value::Sequence(vec![Value::Mapping(entry)]));
    collection.doc.save().report_err("remember where the OpenAPI spec is");
}
