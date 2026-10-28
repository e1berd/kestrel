use std::borrow::Cow;

use gpui::{AssetSource, SharedString};

gpui_kit_assets::icon_assets!(
    ExtraIcons,
    [
        GitBranch,
        Clock,
        Command,
        Send,
        Bird,
        Search,
        CircleCheck,
        CircleX,
        ArrowRight,
        Copy,
        Folder,
        FolderOpen,
        Settings,
        Plus,
        RefreshCw,
        Globe,
        Minus,
        Palette,
        Type,
        Accessibility,
        Keyboard,
        Network,
        SquarePen
    ]
);

pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit_assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let mut paths = gpui_kit_assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}
