pub mod adaptive;
pub mod blockkit;
pub mod messagecard;
pub mod mrkdwn;

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, Default, ToSchema)]
pub struct RenderedMessage {
    pub title: Option<String>,
    /// Nested block tree; typed as objects in OpenAPI to avoid recursive schema expansion.
    #[schema(value_type = Vec<Object>)]
    pub blocks: Vec<RenderBlock>,
    pub username: Option<String>,
    pub icon_url: Option<String>,
    pub icon_emoji: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RenderBlock {
    Header {
        text: String,
    },
    Text {
        html: String,
        plain: String,
    },
    Fields {
        fields: Vec<Field>,
    },
    Image {
        url: String,
        alt: Option<String>,
    },
    Divider,
    Actions {
        actions: Vec<Action>,
    },
    Context {
        elements: Vec<String>,
    },
    Columns {
        columns: Vec<Vec<RenderBlock>>,
    },
    Unsupported {
        kind: String,
        raw: String,
    },
    Code {
        language: Option<String>,
        text: String,
    },
    FactSet {
        facts: Vec<Field>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Field {
    pub title: String,
    pub value_html: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub label: String,
    pub style: Option<String>,
    pub url: Option<String>,
}

impl RenderedMessage {
    pub fn plain_text(text: impl Into<String>) -> Self {
        let plain = text.into();
        let html = html_escape(&plain).replace('\n', "<br>");
        Self {
            blocks: vec![RenderBlock::Text {
                html,
                plain: plain.clone(),
            }],
            ..Default::default()
        }
    }

    pub fn from_markdown(md: &str) -> Self {
        Self {
            blocks: vec![RenderBlock::Text {
                html: markdown_to_html(md),
                plain: md.to_string(),
            }],
            ..Default::default()
        }
    }

    pub fn summary(&self) -> String {
        if let Some(t) = &self.title {
            return t.clone();
        }
        for b in &self.blocks {
            match b {
                RenderBlock::Header { text } => return text.clone(),
                RenderBlock::Text { plain, .. } => {
                    let line = plain.lines().next().unwrap_or("").trim();
                    if !line.is_empty() {
                        return if line.len() > 80 {
                            format!("{}…", &line[..80])
                        } else {
                            line.to_string()
                        };
                    }
                }
                _ => {}
            }
        }
        "(empty message)".into()
    }
}

pub fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn markdown_to_html(md: &str) -> String {
    let mut opts = pulldown_cmark::Options::empty();
    opts.insert(pulldown_cmark::Options::ENABLE_TABLES);
    opts.insert(pulldown_cmark::Options::ENABLE_STRIKETHROUGH);
    let parser = pulldown_cmark::Parser::new_ext(md, opts);
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    html
}
