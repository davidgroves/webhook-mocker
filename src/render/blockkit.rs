use serde_json::Value;

use super::{Action, Field, RenderBlock, RenderedMessage, html_escape, mrkdwn::mrkdwn_to_html};

pub struct BlockKitParse {
    pub message: RenderedMessage,
    pub warnings: Vec<String>,
}

pub fn parse_slack_payload(v: &Value) -> BlockKitParse {
    let mut warnings = Vec::new();
    let mut message = RenderedMessage::default();

    if let Some(u) = v.get("username").and_then(|x| x.as_str()) {
        message.username = Some(u.to_string());
    }
    if let Some(u) = v.get("icon_url").and_then(|x| x.as_str()) {
        message.icon_url = Some(u.to_string());
    }
    if let Some(u) = v.get("icon_emoji").and_then(|x| x.as_str()) {
        message.icon_emoji = Some(u.to_string());
    }

    let text = v.get("text").and_then(|x| x.as_str());
    let blocks = v.get("blocks").and_then(|x| x.as_array());
    let attachments = v.get("attachments").and_then(|x| x.as_array());

    if text.is_none() && blocks.is_none() && attachments.is_none() {
        warnings.push("missing text, blocks, or attachments".into());
    }

    if let Some(t) = text {
        if t.len() > 40_000 {
            warnings.push("Slack would truncate text over 40k chars".into());
        }
        if blocks.is_some() {
            // text is fallback
        } else {
            message.blocks.push(RenderBlock::Text {
                html: mrkdwn_to_html(t),
                plain: t.to_string(),
            });
        }
    }

    if blocks.is_some() && text.is_none() {
        warnings.push("missing `text` fallback alongside `blocks`".into());
    }

    if let Some(blocks) = blocks {
        for b in blocks {
            message.blocks.extend(parse_block(b, &mut warnings));
        }
    }

    if let Some(atts) = attachments {
        for a in atts {
            if let Some(pretext) = a.get("pretext").and_then(|x| x.as_str()) {
                message.blocks.push(RenderBlock::Text {
                    html: mrkdwn_to_html(pretext),
                    plain: pretext.to_string(),
                });
            }
            if let Some(title) = a.get("title").and_then(|x| x.as_str()) {
                message.blocks.push(RenderBlock::Header {
                    text: title.to_string(),
                });
            }
            if let Some(t) = a.get("text").and_then(|x| x.as_str()) {
                message.blocks.push(RenderBlock::Text {
                    html: mrkdwn_to_html(t),
                    plain: t.to_string(),
                });
            }
            if let Some(fields) = a.get("fields").and_then(|x| x.as_array()) {
                let fields: Vec<Field> = fields
                    .iter()
                    .filter_map(|f| {
                        let title = f.get("title")?.as_str()?.to_string();
                        let value = f.get("value")?.as_str().unwrap_or("");
                        Some(Field {
                            title,
                            value_html: mrkdwn_to_html(value),
                        })
                    })
                    .collect();
                if !fields.is_empty() {
                    message.blocks.push(RenderBlock::Fields { fields });
                }
            }
        }
    }

    if message.blocks.is_empty() {
        if let Some(t) = text {
            message.blocks.push(RenderBlock::Text {
                html: mrkdwn_to_html(t),
                plain: t.to_string(),
            });
        } else {
            message.blocks.push(RenderBlock::Text {
                html: "<em>(empty)</em>".into(),
                plain: String::new(),
            });
        }
    }

    BlockKitParse { message, warnings }
}

fn parse_block(b: &Value, warnings: &mut Vec<String>) -> Vec<RenderBlock> {
    let kind = b.get("type").and_then(|x| x.as_str()).unwrap_or("unknown");
    match kind {
        "header" => {
            let text = extract_text(b.get("text")).unwrap_or_default();
            vec![RenderBlock::Header { text }]
        }
        "section" => {
            let mut out = Vec::new();
            if let Some(text) = extract_text(b.get("text")) {
                out.push(RenderBlock::Text {
                    html: mrkdwn_to_html(&text),
                    plain: text,
                });
            }
            if let Some(fields) = b.get("fields").and_then(|x| x.as_array()) {
                let fields: Vec<Field> = fields
                    .iter()
                    .enumerate()
                    .map(|(i, f)| {
                        let value = extract_text(Some(f)).unwrap_or_default();
                        Field {
                            title: format!("Field {}", i + 1),
                            value_html: mrkdwn_to_html(&value),
                        }
                    })
                    .collect();
                out.push(RenderBlock::Fields { fields });
            }
            out
        }
        "divider" => vec![RenderBlock::Divider],
        "image" => {
            let url = b
                .get("image_url")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let alt = b
                .get("alt_text")
                .and_then(|x| x.as_str())
                .map(str::to_string);
            vec![RenderBlock::Image { url, alt }]
        }
        "actions" => {
            let actions = b
                .get("elements")
                .and_then(|x| x.as_array())
                .map(|els| {
                    els.iter()
                        .map(|el| Action {
                            label: extract_text(el.get("text")).unwrap_or_else(|| "Button".into()),
                            style: el.get("style").and_then(|x| x.as_str()).map(str::to_string),
                            url: el.get("url").and_then(|x| x.as_str()).map(str::to_string),
                        })
                        .collect()
                })
                .unwrap_or_default();
            vec![RenderBlock::Actions { actions }]
        }
        "context" => {
            let elements = b
                .get("elements")
                .and_then(|x| x.as_array())
                .map(|els| els.iter().filter_map(|el| extract_text(Some(el))).collect())
                .unwrap_or_default();
            vec![RenderBlock::Context { elements }]
        }
        "rich_text" => {
            let plain = flatten_rich_text(b);
            vec![RenderBlock::Text {
                html: html_escape(&plain).replace('\n', "<br>"),
                plain,
            }]
        }
        other => {
            warnings.push(format!("unknown Block Kit element type `{other}`"));
            vec![RenderBlock::Unsupported {
                kind: other.to_string(),
                raw: b.to_string(),
            }]
        }
    }
}

fn extract_text(v: Option<&Value>) -> Option<String> {
    let v = v?;
    if let Some(s) = v.as_str() {
        return Some(s.to_string());
    }
    if let Some(s) = v.get("text").and_then(|x| x.as_str()) {
        return Some(s.to_string());
    }
    None
}

fn flatten_rich_text(b: &Value) -> String {
    let mut out = String::new();
    if let Some(els) = b.get("elements").and_then(|x| x.as_array()) {
        for el in els {
            if let Some(inner) = el.get("elements").and_then(|x| x.as_array()) {
                for t in inner {
                    if let Some(s) = t.get("text").and_then(|x| x.as_str()) {
                        out.push_str(s);
                    }
                }
                out.push('\n');
            }
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_simple_text() {
        let p = parse_slack_payload(&json!({"text": "hello *there*"}));
        assert!(p.warnings.is_empty());
        assert_eq!(p.message.blocks.len(), 1);
    }

    #[test]
    fn warns_missing_fallback() {
        let p = parse_slack_payload(&json!({
            "blocks": [{"type":"section","text":{"type":"mrkdwn","text":"hi"}}]
        }));
        assert!(p.warnings.iter().any(|w| w.contains("fallback")));
    }
}
