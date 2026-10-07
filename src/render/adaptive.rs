use serde_json::Value;

use super::{Action, Field, RenderBlock, RenderedMessage, html_escape, markdown_to_html};

pub struct AdaptiveParse {
    pub message: RenderedMessage,
    pub warnings: Vec<String>,
}

pub fn parse_adaptive_card(card: &Value) -> AdaptiveParse {
    let mut warnings = Vec::new();
    let mut message = RenderedMessage::default();

    let version = card
        .get("version")
        .and_then(|x| x.as_str())
        .unwrap_or("1.0");
    if version_gt(version, "1.5") {
        warnings.push(format!(
            "Adaptive Card version {version} may not be fully supported by Teams"
        ));
    }

    if let Some(body) = card.get("body").and_then(|x| x.as_array()) {
        for el in body {
            message.blocks.extend(parse_element(el, &mut warnings));
        }
    }

    if let Some(actions) = card.get("actions").and_then(|x| x.as_array()) {
        let actions: Vec<Action> = actions
            .iter()
            .map(|a| Action {
                label: a
                    .get("title")
                    .and_then(|x| x.as_str())
                    .unwrap_or("Action")
                    .to_string(),
                style: a.get("style").and_then(|x| x.as_str()).map(str::to_string),
                url: a.get("url").and_then(|x| x.as_str()).map(str::to_string),
            })
            .collect();
        if !actions.is_empty() {
            message.blocks.push(RenderBlock::Actions { actions });
        }
    }

    if message.blocks.is_empty() {
        message.blocks.push(RenderBlock::Text {
            html: "<em>(empty adaptive card)</em>".into(),
            plain: String::new(),
        });
    }

    AdaptiveParse { message, warnings }
}

pub fn parse_teams_workflows_payload(v: &Value) -> AdaptiveParse {
    let mut warnings = Vec::new();
    // Envelope: { type: message, attachments: [{ contentType, content }] }
    if let Some(atts) = v.get("attachments").and_then(|x| x.as_array()) {
        let mut message = RenderedMessage::default();
        for a in atts {
            let ctype = a.get("contentType").and_then(|x| x.as_str()).unwrap_or("");
            if ctype.contains("adaptive") {
                if let Some(content) = a.get("content") {
                    let mut parsed = parse_adaptive_card(content);
                    warnings.append(&mut parsed.warnings);
                    message.blocks.extend(parsed.message.blocks);
                    if message.title.is_none() {
                        message.title = parsed.message.title;
                    }
                }
            } else {
                warnings.push(format!("unsupported attachment contentType `{ctype}`"));
            }
        }
        if message.blocks.is_empty() {
            warnings.push("no adaptive card attachments found".into());
            message = RenderedMessage::from_markdown(&v.to_string());
        }
        return AdaptiveParse { message, warnings };
    }

    // Bare adaptive card
    if v.get("type").and_then(|x| x.as_str()) == Some("AdaptiveCard") || v.get("$schema").is_some()
    {
        return parse_adaptive_card(v);
    }

    warnings.push("payload is not a Teams workflows message envelope".into());
    AdaptiveParse {
        message: RenderedMessage::from_markdown(
            v.get("text")
                .and_then(|x| x.as_str())
                .unwrap_or(&v.to_string()),
        ),
        warnings,
    }
}

fn parse_element(el: &Value, warnings: &mut Vec<String>) -> Vec<RenderBlock> {
    let kind = el.get("type").and_then(|x| x.as_str()).unwrap_or("unknown");
    match kind {
        "TextBlock" => {
            let text = el.get("text").and_then(|x| x.as_str()).unwrap_or("");
            let weight = el.get("weight").and_then(|x| x.as_str()).unwrap_or("");
            let size = el.get("size").and_then(|x| x.as_str()).unwrap_or("");
            if weight.eq_ignore_ascii_case("bolder") || size.eq_ignore_ascii_case("large") {
                vec![RenderBlock::Header {
                    text: text.to_string(),
                }]
            } else {
                vec![RenderBlock::Text {
                    html: markdown_to_html(text),
                    plain: text.to_string(),
                }]
            }
        }
        "Image" => {
            let url = el
                .get("url")
                .and_then(|x| x.as_str())
                .unwrap_or("")
                .to_string();
            let alt = el
                .get("altText")
                .and_then(|x| x.as_str())
                .map(str::to_string);
            vec![RenderBlock::Image { url, alt }]
        }
        "FactSet" => {
            let facts = el
                .get("facts")
                .and_then(|x| x.as_array())
                .map(|facts| {
                    facts
                        .iter()
                        .map(|f| Field {
                            title: f
                                .get("title")
                                .and_then(|x| x.as_str())
                                .unwrap_or("")
                                .to_string(),
                            value_html: html_escape(
                                f.get("value").and_then(|x| x.as_str()).unwrap_or(""),
                            ),
                        })
                        .collect()
                })
                .unwrap_or_default();
            vec![RenderBlock::FactSet { facts }]
        }
        "ColumnSet" => {
            let columns = el
                .get("columns")
                .and_then(|x| x.as_array())
                .map(|cols| {
                    cols.iter()
                        .map(|c| {
                            c.get("items")
                                .and_then(|x| x.as_array())
                                .map(|items| {
                                    items
                                        .iter()
                                        .flat_map(|i| parse_element(i, warnings))
                                        .collect()
                                })
                                .unwrap_or_default()
                        })
                        .collect()
                })
                .unwrap_or_default();
            vec![RenderBlock::Columns { columns }]
        }
        "Container" => el
            .get("items")
            .and_then(|x| x.as_array())
            .map(|items| {
                items
                    .iter()
                    .flat_map(|i| parse_element(i, warnings))
                    .collect()
            })
            .unwrap_or_default(),
        "ActionSet" => {
            let actions = el
                .get("actions")
                .and_then(|x| x.as_array())
                .map(|acts| {
                    acts.iter()
                        .map(|a| Action {
                            label: a
                                .get("title")
                                .and_then(|x| x.as_str())
                                .unwrap_or("Action")
                                .to_string(),
                            style: a.get("style").and_then(|x| x.as_str()).map(str::to_string),
                            url: a.get("url").and_then(|x| x.as_str()).map(str::to_string),
                        })
                        .collect()
                })
                .unwrap_or_default();
            vec![RenderBlock::Actions { actions }]
        }
        other => {
            warnings.push(format!("unsupported Adaptive Card element `{other}`"));
            vec![RenderBlock::Unsupported {
                kind: other.to_string(),
                raw: el.to_string(),
            }]
        }
    }
}

fn version_gt(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> (u32, u32) {
        let mut parts = s.split('.');
        let major = parts.next().and_then(|x| x.parse().ok()).unwrap_or(0);
        let minor = parts.next().and_then(|x| x.parse().ok()).unwrap_or(0);
        (major, minor)
    };
    parse(a) > parse(b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_textblock() {
        let p = parse_adaptive_card(&json!({
            "type": "AdaptiveCard",
            "version": "1.4",
            "body": [{"type":"TextBlock","text":"Hello"}]
        }));
        assert!(p.warnings.is_empty());
        assert!(!p.message.blocks.is_empty());
    }
}
