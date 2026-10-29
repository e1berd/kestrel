use anyhow::{Context as _, Result, bail};
use serde_json::Value as Json;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    OpenApi,
    Apidog,
}

impl Format {
    pub const ALL: [Self; 2] = [Self::OpenApi, Self::Apidog];

    pub fn id(self) -> &'static str {
        match self {
            Self::OpenApi => "openapi",
            Self::Apidog => "apidog",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::OpenApi => "OpenAPI / Swagger",
            Self::Apidog => "Apidog",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::OpenApi => "OpenAPI 3.0, 3.1, 3.2 and Swagger 2.0 · JSON or YAML",
            Self::Apidog => "Native project export · JSON",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        Self::ALL.into_iter().find(|format| format.id() == value).with_context(|| format!("unknown import format: {value}"))
    }

    pub fn detect(document: &Json) -> Result<Self> {
        if document.get("openapi").is_some() || document.get("swagger").is_some() {
            return Ok(Self::OpenApi);
        }
        if document.get("apidogProject").is_some() || document.get("apiCollection").is_some() || document.get("httpCollection").is_some() {
            return Ok(Self::Apidog);
        }
        bail!("unrecognized import format; choose an OpenAPI / Swagger spec or an Apidog project export")
    }
}
