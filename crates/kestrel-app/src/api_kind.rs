use gpui::{App, Styled as _};
use gpui_component::Icon;
use kestrel_core::format::RequestFile;

use crate::appearance::{self, dp};
use crate::assets;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ApiKind {
    Rest,
    GraphQl,
    RabbitMq,
}

impl ApiKind {
    pub fn of(file: &RequestFile) -> Self {
        if file.rabbitmq.is_some() {
            Self::RabbitMq
        } else if file.http.as_ref().and_then(|http| http.body.as_ref()).is_some_and(|body| body.kind == "graphql") {
            Self::GraphQl
        } else {
            Self::Rest
        }
    }

    pub fn from_method(method: &str) -> Option<Self> {
        match method {
            "GQL" => Some(Self::GraphQl),
            "RMQ" => Some(Self::RabbitMq),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Rest => "REST",
            Self::GraphQl => "GraphQL",
            Self::RabbitMq => "RabbitMQ",
        }
    }

    pub fn icon(self, cx: &App) -> Icon {
        let palette = appearance::palette(cx);
        match self {
            Self::Rest => Icon::new(gpui_kit_assets::IconName::Globe).text_color(palette.muted),
            Self::GraphQl => Icon::default().path(assets::GRAPHQL_LOGO).text_color(palette.accent),
            Self::RabbitMq => Icon::default().path(assets::RABBITMQ_LOGO).text_color(palette.warning),
        }
        .size(dp(18.))
    }
}
