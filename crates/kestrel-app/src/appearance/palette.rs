use std::path::{Path, PathBuf};

use gpui::{Hsla, Rgba};
use kestrel_core::report::report_error;
use serde::{Deserialize, Deserializer};

const BUILT_IN: [(&str, &str); 3] = [
    ("kestrel", include_str!("../../assets/themes/kestrel.json")),
    ("fjord", include_str!("../../assets/themes/fjord.json")),
    ("sepia", include_str!("../../assets/themes/sepia.json")),
];

pub const DEFAULT_SCHEME: &str = "kestrel";

#[derive(Clone, Debug, Deserialize)]
pub struct ColorScheme {
    #[serde(skip)]
    pub id: String,
    #[serde(skip)]
    pub file: Option<PathBuf>,
    pub name: String,
    pub light: Palette,
    pub dark: Palette,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(from = "PaletteFile")]
pub struct Palette {
    pub background: Hsla,
    pub surface: Hsla,
    pub raised: Hsla,
    pub border: Hsla,
    pub border_soft: Hsla,
    pub hover: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub faint: Hsla,
    pub code: Hsla,
    pub accent: Hsla,
    pub accent_text: Hsla,
    pub selection: Hsla,
    pub success: Hsla,
    pub warning: Hsla,
    pub danger: Hsla,
    pub info: Hsla,
    pub methods: MethodColors,
    pub syntax: Option<SyntaxPalette>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SyntaxPalette {
    pub property: String,
    pub string: String,
    pub number: String,
    pub keyword: String,
    pub punctuation: String,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub function: Option<String>,
}

#[derive(Deserialize)]
struct PaletteFile {
    #[serde(deserialize_with = "color")]
    background: Hsla,
    #[serde(deserialize_with = "color")]
    surface: Hsla,
    #[serde(deserialize_with = "color")]
    raised: Hsla,
    #[serde(deserialize_with = "color")]
    border: Hsla,
    #[serde(default, deserialize_with = "optional_color")]
    border_soft: Option<Hsla>,
    #[serde(deserialize_with = "color")]
    hover: Hsla,
    #[serde(deserialize_with = "color")]
    text: Hsla,
    #[serde(deserialize_with = "color")]
    muted: Hsla,
    #[serde(default, deserialize_with = "optional_color")]
    faint: Option<Hsla>,
    #[serde(default, deserialize_with = "optional_color")]
    code: Option<Hsla>,
    #[serde(deserialize_with = "color")]
    accent: Hsla,
    #[serde(deserialize_with = "color")]
    accent_text: Hsla,
    #[serde(deserialize_with = "color")]
    selection: Hsla,
    #[serde(deserialize_with = "color")]
    success: Hsla,
    #[serde(deserialize_with = "color")]
    warning: Hsla,
    #[serde(deserialize_with = "color")]
    danger: Hsla,
    #[serde(deserialize_with = "color")]
    info: Hsla,
    methods: MethodColors,
    #[serde(default)]
    syntax: Option<SyntaxPalette>,
}

impl From<PaletteFile> for Palette {
    fn from(file: PaletteFile) -> Self {
        Self {
            border_soft: file.border_soft.unwrap_or_else(|| mix(file.border, file.background, 0.45)),
            faint: file.faint.unwrap_or_else(|| mix(file.muted, file.background, 0.18)),
            code: file.code.unwrap_or_else(|| mix(file.background, file.surface, 0.35)),
            background: file.background,
            surface: file.surface,
            raised: file.raised,
            border: file.border,
            hover: file.hover,
            text: file.text,
            muted: file.muted,
            accent: file.accent,
            accent_text: file.accent_text,
            selection: file.selection,
            success: file.success,
            warning: file.warning,
            danger: file.danger,
            info: file.info,
            methods: file.methods,
            syntax: file.syntax,
        }
    }
}

impl Palette {
    pub fn tint(&self, color: Hsla) -> Hsla {
        let is_dark = self.background.l < 0.5;
        Hsla { a: if is_dark { 0.14 } else { 0.1 }, ..color }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct MethodColors {
    #[serde(deserialize_with = "color")]
    pub get: Hsla,
    #[serde(deserialize_with = "color")]
    pub post: Hsla,
    #[serde(deserialize_with = "color")]
    pub put: Hsla,
    #[serde(deserialize_with = "color")]
    pub patch: Hsla,
    #[serde(deserialize_with = "color")]
    pub delete: Hsla,
    #[serde(deserialize_with = "color")]
    pub other: Hsla,
}

impl MethodColors {
    pub fn of(&self, method: &str) -> Hsla {
        match method {
            "GET" | "QUERY" => self.get,
            "POST" => self.post,
            "PUT" => self.put,
            "PATCH" => self.patch,
            "DELETE" => self.delete,
            _ => self.other,
        }
    }
}

fn optional_color<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Hsla>, D::Error> {
    color(deserializer).map(Some)
}

fn color<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Hsla, D::Error> {
    let text = String::deserialize(deserializer)?;
    parse_hex(&text).ok_or_else(|| serde::de::Error::custom(format!("`{text}` is not a #rrggbb or #rrggbbaa colour")))
}

pub fn parse_hex(text: &str) -> Option<Hsla> {
    let digits = text.strip_prefix('#')?;
    let value = u32::from_str_radix(digits, 16).ok()?;
    let rgba = match digits.len() {
        6 => (value << 8) | 0xFF,
        8 => value,
        _ => return None,
    };
    Some(gpui::rgba(rgba).into())
}

pub fn mix(base: Hsla, other: Hsla, amount: f32) -> Hsla {
    let a = Rgba::from(base);
    let b = Rgba::from(other);
    let lerp = |x: f32, y: f32| x + (y - x) * amount;
    Rgba { r: lerp(a.r, b.r), g: lerp(a.g, b.g), b: lerp(a.b, b.b), a: lerp(a.a, b.a) }.into()
}

pub fn user_schemes_dir() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("kestrel").join("themes"))
}

pub fn load_all() -> Vec<ColorScheme> {
    let mut schemes: Vec<ColorScheme> = BUILT_IN.iter().filter_map(|(id, text)| parse_scheme(id, text, None)).collect();
    if let Some(dir) = user_schemes_dir() {
        schemes.extend(read_user_schemes(&dir));
    }
    schemes
}

pub fn built_in_source(id: &str) -> Option<&'static str> {
    BUILT_IN.iter().find(|(built_in, _)| *built_in == id).map(|(_, text)| *text)
}

fn read_user_schemes(dir: &Path) -> Vec<ColorScheme> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut paths: Vec<PathBuf> =
        entries.filter_map(Result::ok).map(|entry| entry.path()).filter(|path| path.extension().is_some_and(|e| e == "json")).collect();
    paths.sort();
    paths
        .into_iter()
        .filter_map(|path| {
            let text = std::fs::read_to_string(&path).map_err(|error| report_error(&format!("read {}", path.display()), error)).ok()?;
            let id = format!("user:{}", path.file_stem()?.to_string_lossy());
            parse_scheme(&id, &text, Some(path))
        })
        .collect()
}

fn parse_scheme(id: &str, text: &str, file: Option<PathBuf>) -> Option<ColorScheme> {
    match serde_json::from_str::<ColorScheme>(text) {
        Ok(mut scheme) => {
            scheme.id = id.to_string();
            scheme.file = file;
            Some(scheme)
        }
        Err(error) => {
            report_error(&format!("read the colour scheme {id}"), error);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_scheme_reads() {
        for (id, text) in BUILT_IN {
            assert!(parse_scheme(id, text, None).is_some(), "{id}");
        }
    }

    #[test]
    fn hex_colours_read_with_and_without_alpha() {
        assert!(parse_hex("#F2995E").is_some());
        assert!(parse_hex("#F2995E33").is_some_and(|c| c.a < 0.25));
        assert!(parse_hex("F2995E").is_none());
    }
}
