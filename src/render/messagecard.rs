use serde_json::Value;

use super::{Action, Field, RenderBlock, RenderedMessage, html_escape, markdown_to_html};
use crate::render::adaptive::{self, AdaptiveParse};

pub fn parse_legacy_teams_payload(v: &Value) -> AdaptiveParse {
    // Adaptive card sent to legacy endpoint
    if v.get("@type").and_then(|x| x.as_str()) == Some("AdaptiveCard")
        || v.get("type").and_then(|x| x.as_str()) == Some("AdaptiveCard")
    {
        return adaptive::parse_adaptive_card(v);
    }

    // MessageCard
    if v.get("@type").and_then(|x| x.as_str()) == Some("MessageCard")
        || v.get("@context").is_some()
        || v.get("summary").is_some()
        || v.get("sections").is_some()
    {
        return parse_message_card(v);
    }

    // Workflows-style envelope mistakenly posted here
    if v.get("attachments").is_some() {
        return adaptive::parse_teams_workflows_payload(v);
    }

    AdaptiveParse {
        message: RenderedMessage::from_markdown(
            v.get("text")
                .and_then(|x| x.as_str())
                .or_else(|| v.get("title").and_then(|x| x.as_str()))
                .unwrap_or(&v.to_string()),
        ),
        warnings: vec!["payload did not match MessageCard or AdaptiveCard".into()],
    }
}

fn parse_message_card(v: &Value) -> AdaptiveParse {
    let mut warnings = Vec::new();
    let mut message = RenderedMessage {
        title: v
            .get("title")
            .and_then(|x| x.as_str())
            .map(str::to_string)
            .or_else(|| {
                v.get("summary")
                    .and_then(|x| x.as_str())
                    .map(str::to_string)
            }),
        ..Default::default()
    };

    if let Some(title) = &message.title {
        message.blocks.push(RenderBlock::Header {
            text: title.clone(),
        });
    }

    if let Some(text) = v.get("text").and_then(|x| x.as_str()) {
        message.blocks.push(RenderBlock::Text {
            html: markdown_to_html(text),
            plain: text.to_string(),
        });
    }

    if let Some(sections) = v.get("sections").and_then(|x| x.as_array()) {
        for s in sections {
            if let Some(at) = s.get("activityTitle").and_then(|x| x.as_str()) {
                message.blocks.push(RenderBlock::Header {
                    text: at.to_string(),
                });
            }
            if let Some(asub) = s.get("activitySubtitle").and_then(|x| x.as_str()) {
                message.blocks.push(RenderBlock::Context {
                    elements: vec![asub.to_string()],
                });
            }
            if let Some(text) = s.get("text").and_then(|x| x.as_str()) {
                message.blocks.push(RenderBlock::Text {
                    html: markdown_to_html(text),
                    plain: text.to_string(),
                });
            }
            if let Some(facts) = s.get("facts").and_then(|x| x.as_array()) {
                let facts: Vec<Field> = facts
                    .iter()
                    .map(|f| Field {
                        title: f
                            .get("name")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .to_string(),
                        value_html: html_escape(
                            f.get("value").and_then(|x| x.as_str()).unwrap_or(""),
                        ),
                    })
                    .collect();
                if !facts.is_empty() {
                    message.blocks.push(RenderBlock::FactSet { facts });
                }
            }
            if s.get("images").is_some() {
                warnings.push("MessageCard section images are not fully rendered".into());
            }
        }
    }

    if let Some(actions) = v
        .get("potentialAction")
        .and_then(|x| x.as_array())
        .or_else(|| v.get("actions").and_then(|x| x.as_array()))
    {
        let actions: Vec<Action> = actions
            .iter()
            .map(|a| {
                let url = a
                    .get("targets")
                    .and_then(|t| t.as_array())
                    .and_then(|t| t.first())
                    .and_then(|t| t.get("uri"))
                    .and_then(|x| x.as_str())
                    .or_else(|| a.get("uri").and_then(|x| x.as_str()))
                    .map(str::to_string);
                Action {
                    label: a
                        .get("name")
                        .and_then(|x| x.as_str())
                        .or_else(|| a.get("title").and_then(|x| x.as_str()))
                        .unwrap_or("Open")
                        .to_string(),
                    style: None,
                    url,
                }
            })
            .collect();
        if !actions.is_empty() {
            message.blocks.push(RenderBlock::Actions { actions });
        }
    }

    if message.blocks.is_empty() {
        message.blocks.push(RenderBlock::Text {
            html: "<em>(empty MessageCard)</em>".into(),
            plain: String::new(),
        });
    }

    AdaptiveParse { message, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_message_card() {
        let p = parse_legacy_teams_payload(&json!({
            "@type": "MessageCard",
            "@context": "https://schema.org/extensions",
            "summary": "Alert",
            "title": "Build failed",
            "text": "Pipeline **broken**"
        }));
        assert!(p.message.title.as_deref() == Some("Build failed"));
    }
}
