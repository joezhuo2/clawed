//! Exact plan usage from the Claude account usage endpoint (what `/usage`
//! shows), authenticated with Claude Code's own OAuth token.
//!
//! The token is read from `<config dir>/.credentials.json` and only ever sent
//! to api.anthropic.com. It is never refreshed here: an expired token is
//! skipped until Claude Code refreshes it.

use std::path::Path;
use std::time::Duration;

use islet_proto::Window;
use serde::Deserialize;

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const BETA: &str = "oauth-2025-04-20";
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Deserialize)]
struct Credentials {
    #[serde(rename = "claudeAiOauth")]
    oauth: Option<OauthToken>,
}

#[derive(Deserialize)]
struct OauthToken {
    #[serde(rename = "accessToken")]
    access_token: String,
    /// Unix epoch milliseconds.
    #[serde(rename = "expiresAt")]
    expires_at: Option<u64>,
}

/// Current access token, if present and not expired.
pub fn read_token(config_dir: &Path, now_ms: u64) -> Option<String> {
    let raw = std::fs::read_to_string(config_dir.join(".credentials.json")).ok()?;
    let tok = serde_json::from_str::<Credentials>(&raw).ok()?.oauth?;
    if tok.expires_at.is_some_and(|e| e <= now_ms) || tok.access_token.is_empty() {
        return None;
    }
    Some(tok.access_token)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApiUsage {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
}

#[derive(Deserialize)]
struct RawUsage {
    five_hour: Option<RawWindow>,
    seven_day: Option<RawWindow>,
}

#[derive(Deserialize)]
struct RawWindow {
    utilization: Option<f64>,
    resets_at: Option<String>,
}

fn window(w: Option<RawWindow>) -> Option<Window> {
    let w = w?;
    Some(Window { used_pct: w.utilization?, resets_at: w.resets_at.as_deref().and_then(parse_rfc3339) })
}

pub fn parse_response(body: &str) -> Option<ApiUsage> {
    let raw: RawUsage = serde_json::from_str(body).ok()?;
    let u = ApiUsage { five_hour: window(raw.five_hour), seven_day: window(raw.seven_day) };
    (u.five_hour.is_some() || u.seven_day.is_some()).then_some(u)
}

/// HTTP agent shared by the usage poll and the update check.
pub(crate) fn agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder().timeout_global(Some(TIMEOUT));
    // macOS uses Security.framework and the keychain's roots, so no C TLS
    // library (ring) has to be built for the target.
    #[cfg(target_os = "macos")]
    let config = config.tls_config(
        ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::NativeTls)
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build(),
    );
    config.build().into()
}

/// Blocking request; call from a blocking thread.
pub fn fetch(token: &str) -> Result<ApiUsage, String> {
    let mut resp = agent()
        .get(USAGE_URL)
        .header("Authorization", &format!("Bearer {token}"))
        .header("anthropic-beta", BETA)
        .header("User-Agent", concat!("islet/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| e.to_string())?;
    let body = resp.body_mut().read_to_string().map_err(|e| e.to_string())?;
    parse_response(&body).ok_or_else(|| "usage response had no rate limit windows".into())
}

/// `2026-10-01T14:40:00.057904+00:00` (or `Z`) to Unix epoch seconds.
pub fn parse_rfc3339(s: &str) -> Option<u64> {
    let b = s.as_bytes();
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || !matches!(b[10], b'T' | b't' | b' ') {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let (y, mo, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, mi, sec) = (num(11..13)?, num(14..16)?, num(17..19)?);
    // Skip fractional seconds, then read the offset.
    let mut i = 19;
    if b.get(i) == Some(&b'.') {
        i += 1;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
    }
    let offset = match b.get(i) {
        None | Some(b'Z' | b'z') => 0,
        Some(&sign @ (b'+' | b'-')) => {
            let oh = num(i + 1..i + 3)?;
            let om = num(i + 4..i + 6)?;
            let o = oh * 3600 + om * 60;
            if sign == b'-' {
                -o
            } else {
                o
            }
        }
        _ => return None,
    };
    let secs = days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + sec - offset;
    u64::try_from(secs).ok()
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc3339() {
        assert_eq!(parse_rfc3339("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339("2026-10-01T14:40:00.057904+00:00"), Some(1_790_865_600));
        assert_eq!(parse_rfc3339("2026-10-01T10:40:00-04:00"), Some(1_790_865_600));
        assert_eq!(parse_rfc3339("2024-02-29T00:00:00Z"), Some(1_709_164_800));
        assert_eq!(parse_rfc3339("nope"), None);
    }

    #[test]
    fn response() {
        let body = r#"{"five_hour":{"utilization":79.0,"resets_at":"2026-10-01T14:40:00.057904+00:00","limit_dollars":null},
            "seven_day":{"utilization":9.0,"resets_at":null},"seven_day_opus":null,"extra":{"x":1}}"#;
        let u = parse_response(body).unwrap();
        assert_eq!(u.five_hour, Some(Window { used_pct: 79.0, resets_at: Some(1_790_865_600) }));
        assert_eq!(u.seven_day, Some(Window { used_pct: 9.0, resets_at: None }));
        assert!(parse_response(r#"{"five_hour":null,"seven_day":null}"#).is_none());
        assert!(parse_response("<html>").is_none());
    }

    #[test]
    fn token() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".credentials.json");
        assert_eq!(read_token(dir.path(), 0), None);
        std::fs::write(&path, r#"{"claudeAiOauth":{"accessToken":"tok","expiresAt":5000}}"#).unwrap();
        assert_eq!(read_token(dir.path(), 4999).as_deref(), Some("tok"));
        assert_eq!(read_token(dir.path(), 5000), None);
    }
}

#[cfg(test)]
mod live {
    /// `cargo test -p islet --lib oauth::live -- --ignored --nocapture`
    #[test]
    #[ignore = "hits the network with the local Claude login"]
    fn fetch_live() {
        let dir = dirs::home_dir().unwrap().join(".claude");
        let tok = super::read_token(&dir, islet_proto::now_ms()).expect("token");
        println!("{:?}", super::fetch(&tok).unwrap());
    }
}
