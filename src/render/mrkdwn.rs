use super::html_escape;

/// Convert Slack mrkdwn to simple HTML.
pub fn mrkdwn_to_html(input: &str) -> String {
    let escaped = html_escape(input);
    let mut out = String::with_capacity(escaped.len());
    let chars: Vec<char> = escaped.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // Links: <url|label> or <url> — but we already escaped < to &lt;
        if chars[i] == '&'
            && escaped[byte_offset(&chars, i)..].starts_with("&lt;")
            && let Some(end) = find_escaped_gt(&chars, i)
        {
            let inner_start = i + 4; // after &lt;
            let inner: String = chars[inner_start..end].iter().collect();
            if let Some((url, label)) = inner.split_once('|') {
                out.push_str(&format!(
                    r#"<a href="{url}" target="_blank" rel="noopener">{label}</a>"#
                ));
            } else if inner.starts_with('#') || inner.starts_with('@') || inner.starts_with('!') {
                out.push_str(&format!("<span class=\"mention\">{inner}</span>"));
            } else {
                out.push_str(&format!(
                    r#"<a href="{inner}" target="_blank" rel="noopener">{inner}</a>"#
                ));
            }
            i = end + 4; // skip &gt;
            continue;
        }

        // Bold *text*
        if chars[i] == '*'
            && let Some(end) = find_delim(&chars, i + 1, '*')
        {
            let inner: String = chars[i + 1..end].iter().collect();
            out.push_str(&format!("<strong>{inner}</strong>"));
            i = end + 1;
            continue;
        }
        // Italic _text_
        if chars[i] == '_'
            && let Some(end) = find_delim(&chars, i + 1, '_')
        {
            let inner: String = chars[i + 1..end].iter().collect();
            out.push_str(&format!("<em>{inner}</em>"));
            i = end + 1;
            continue;
        }
        // Strike ~text~
        if chars[i] == '~'
            && let Some(end) = find_delim(&chars, i + 1, '~')
        {
            let inner: String = chars[i + 1..end].iter().collect();
            out.push_str(&format!("<del>{inner}</del>"));
            i = end + 1;
            continue;
        }
        // Code `text`
        if chars[i] == '`'
            && !escaped[byte_offset(&chars, i)..].starts_with("```")
            && let Some(end) = find_delim(&chars, i + 1, '`')
        {
            let inner: String = chars[i + 1..end].iter().collect();
            out.push_str(&format!("<code>{inner}</code>"));
            i = end + 1;
            continue;
        }
        // Code block ```
        if chars[i] == '`'
            && i + 2 < chars.len()
            && chars[i + 1] == '`'
            && chars[i + 2] == '`'
            && let Some(end) = find_triple_backtick(&chars, i + 3)
        {
            let inner: String = chars[i + 3..end].iter().collect();
            out.push_str(&format!(
                "<pre><code>{}</code></pre>",
                inner.trim_start_matches('\n')
            ));
            i = end + 3;
            continue;
        }

        if chars[i] == '\n' {
            out.push_str("<br>");
        } else {
            out.push(chars[i]);
        }
        i += 1;
    }
    out
}

fn byte_offset(chars: &[char], idx: usize) -> usize {
    chars[..idx].iter().map(|c| c.len_utf8()).sum()
}

fn find_delim(chars: &[char], start: usize, delim: char) -> Option<usize> {
    let mut i = start;
    while i < chars.len() {
        if chars[i] == delim {
            return Some(i);
        }
        if chars[i] == '\n' {
            return None;
        }
        i += 1;
    }
    None
}

fn find_triple_backtick(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 2 < chars.len() {
        if chars[i] == '`' && chars[i + 1] == '`' && chars[i + 2] == '`' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn find_escaped_gt(chars: &[char], start: usize) -> Option<usize> {
    // find &gt; after start
    let mut i = start;
    while i + 3 < chars.len() {
        if chars[i] == '&' && chars[i + 1] == 'g' && chars[i + 2] == 't' && chars[i + 3] == ';' {
            return Some(i);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bold_and_link() {
        let html = mrkdwn_to_html("Hello *world*");
        assert!(html.contains("<strong>world</strong>"));
    }
}
