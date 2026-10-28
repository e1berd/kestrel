use std::path::{Path, PathBuf};

use gpui::{Hsla, Rgba};
use serde::{Deserialize, Deserializer};
use volley_core::report::report_error;

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
pub struct Palette {
    #[serde(deserialize_with = "color")]
    pub background: Hsla,
    #[serde(deserialize_with = "color")]
    pub surface: Hsla,
    #[serde(deserialize_with = "color")]
    pub raised: Hsla,
    #[serde(deserialize_with = "color")]
    pub border: Hsla,
    #[serde(deserialize_with = "color")]
    pub hover: Hsla,
    #[serde(deserialize_with = "color")]
    pub text: Hsla,
    #[serde(deserialize_with = "color")]
    pub muted: Hsla,
    #[serde(deserialize_with = "color")]
    pub accent: Hsla,
    #[serde(deserialize_with = "color")]
    pub accent_text: Hsla,
    #[serde(deserialize_with = "color")]
    pub selection: Hsla,
    #[serde(deserialize_with = "color")]
    pub success: Hsla,
    #[serde(deserialize_with = "color")]
    pub warning: Hsla,
    #[serde(deserialize_with = "color")]
    pub danger: Hsla,
    #[serde(deserialize_with = "color")]
    pub info: Hsla,
    pub methods: MethodColors,
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
            "GET" => self.get,
            "POST" => self.post,
            "PUT" => self.put,
            "PATCH" => self.patch,
            "DELETE" => self.delete,
            _ => self.other,
        }
    }
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
