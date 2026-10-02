//! Minimal parsing of Claude Code transcript JSONL lines: `usage` and the
//! assistant's latest text (what Claude says it is doing).

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq)]
pub struct UsageLine {
    pub msg_id: Option<String>,
    pub ts_ms: Option<u64>,
    pub model: Option<String>,
    pub sidechain: bool,
    pub input: u64,
    pub cache_creation: u64,
    pub cache_read: u64,
    pub output: u64,
}

impl UsageLine {
    /// Tokens occupying the context window after this response.
    pub fn context_tokens(&self) -> u64 {
        self.input.saturating_add(self.cache_creation).saturating_add(self.cache_read)
    }

    /// Tokens counted toward the plan usage estimate. Cache reads are
    /// excluded since they are far cheaper than fresh input.
    pub fn billable_tokens(&self) -> u64 {
        self.input.saturating_add(self.cache_creation).saturating_add(self.output)
    }
}

#[derive(Deserialize)]
struct RawLine {
    #[serde(rename = "type")]
    kind: Option<String>,
    timestamp: Option<String>,
    #[serde(rename = "isSidechain", default)]
    sidechain: bool,
    message: Option<RawMessage>,
}

#[derive(Deserialize)]
struct RawMessage {
    id: Option<String>,
    model: Option<String>,
    usage: Option<RawUsage>,
}

#[derive(Deserialize, Default)]
struct RawUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}

/// Parses one transcript line. Cheap pre-check skips lines without usage.
pub fn parse_line(line: &str) -> Option<UsageLine> {
    if !line.contains("\"usage\"") {
        return None;
    }
    let raw: RawLine = serde_json::from_str(line).ok()?;
    if raw.kind.as_deref() != Some("assistant") {
        return None;
    }
    let msg = raw.message?;
    let u = msg.usage?;
    Some(UsageLine {
        msg_id: msg.id,
        ts_ms: raw.timestamp.as_deref().and_then(parse_iso_ms),
        model: msg.model,
        sidechain: raw.sidechain,
        input: u.input_tokens,
        cache_creation: u.cache_creation_input_tokens,
        cache_read: u.cache_read_input_tokens,
        output: u.output_tokens,
    })
}

const NARRATION_MAX: usize = 200;

#[derive(Deserialize)]
struct TextLine {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(rename = "isSidechain", default)]
    sidechain: bool,
    message: Option<TextMessage>,
}

#[derive(Deserialize)]
struct TextMessage {
    #[serde(default)]
    content: serde_json::Value,
}

/// Last text block of a main-thread assistant line, whitespace collapsed and
/// truncated. Subagent (sidechain) text is skipped.
pub fn parse_text(line: &str) -> Option<String> {
    if !line.contains("\"text\"") {
        return None;
    }
    let raw: TextLine = serde_json::from_str(line).ok()?;
    if raw.kind.as_deref() != Some("assistant") || raw.sidechain {
        return None;
    }
    let content = raw.message?.content;
    let text = content
        .as_array()?
        .iter()
        .rev()
        .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
        .find_map(|b| b.get("text").and_then(|t| t.as_str()))?;
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    (!text.is_empty()).then(|| islet_proto::truncate(&text, NARRATION_MAX))
}

/// Parses `YYYY-MM-DDTHH:MM:SS[.fff]Z` to Unix milliseconds.
pub fn parse_iso_ms(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() < 20 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let mut ms = 0;
    if b.get(19) == Some(&b'.') {
        let frac: String = s[20..].chars().take_while(char::is_ascii_digit).take(3).collect();
        ms = format!("{frac:0<3}").parse::<i64>().ok()?;
    }
    // Days from civil (Howard Hinnant).
    let y = if mo <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (mo + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    let total = ((days * 24 + h) * 60 + mi) * 60 + sec;
    u64::try_from(total * 1000 + ms).ok()
}

/// Context window size for a model id, honoring user overrides.
pub fn window_for_model(model: &str, overrides: &std::collections::HashMap<String, u64>) -> u64 {
    if let Some((_, w)) = overrides.iter().find(|(k, _)| model.contains(k.as_str())) {
        return *w;
    }
    if model.contains("[1m]") || model.ends_with("-1m") {
        1_000_000
    } else {
        200_000
    }
}

/// A transcript can't hold more tokens than its window, so usage above the
/// assumed window means an extended-context session.
pub fn effective_window(tokens: u64, window: u64) -> u64 {
    if tokens > window && window < 1_000_000 {
        1_000_000
    } else {
        window
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_grows_when_exceeded() {
        assert_eq!(effective_window(150_000, 200_000), 200_000);
        assert_eq!(effective_window(250_000, 200_000), 1_000_000);
    }

    const LINE: &str = r#"{"type":"assistant","isSidechain":false,"timestamp":"2026-09-30T19:59:45.666Z","message":{"id":"msg_1","model":"claude-opus-5-5","usage":{"input_tokens":2,"cache_creation_input_tokens":57126,"cache_read_input_tokens":39921,"output_tokens":171}}}"#;

    #[test]
    fn parse_usage_line_assistant() {
        let u = parse_line(LINE).unwrap();
        assert_eq!(u.context_tokens(), 2 + 57126 + 39921);
        assert_eq!(u.billable_tokens(), 2 + 57126 + 171);
        assert_eq!(u.msg_id.as_deref(), Some("msg_1"));
        assert_eq!(u.ts_ms, Some(1_790_798_385_666));
    }

    #[test]
    fn non_usage_line_none() {
        assert!(parse_line(r#"{"type":"user","message":{"content":"hi"}}"#).is_none());
        assert!(parse_line(r#"{"type":"user","message":{"usage":{}}}"#).is_none());
        assert!(parse_line("garbage \"usage\"").is_none());
    }

    #[test]
    fn text_from_assistant_only() {
        let a = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Let me\n  run the tests."},{"type":"tool_use","name":"Bash"}]}}"#;
        assert_eq!(parse_text(a).as_deref(), Some("Let me run the tests."));
        let side = a.replace(r#""type":"assistant""#, r#""type":"assistant","isSidechain":true"#);
        assert!(parse_text(&side).is_none());
        assert!(parse_text(r#"{"type":"user","message":{"content":[{"type":"text","text":"hi"}]}}"#).is_none());
        assert!(parse_text(r#"{"type":"assistant","message":{"content":[{"type":"text","text":"  "}]}}"#).is_none());
        assert!(parse_text(LINE).is_none());
    }

    #[test]
    fn iso_parsing() {
        assert_eq!(parse_iso_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso_ms("2000-03-01T00:00:00.5Z"), Some(951_868_800_500));
        assert_eq!(parse_iso_ms("nope"), None);
    }

    #[test]
    fn model_windows() {
        let mut o = std::collections::HashMap::new();
        assert_eq!(window_for_model("claude-opus-5-5", &o), 200_000);
        assert_eq!(window_for_model("claude-opus-5-5[1m]", &o), 1_000_000);
        o.insert("opus".to_string(), 500_000);
        assert_eq!(window_for_model("claude-opus-5-5", &o), 500_000);
    }
}
