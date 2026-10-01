//! Update notice: asks GitHub for the latest published release and, when it is
//! newer than this build, the tray offers to open its release page. Nothing is
//! downloaded or installed; the user runs the new installer themselves.

use serde::Deserialize;

const LATEST_URL: &str = "https://api.github.com/repos/joezhuo2/clawed/releases/latest";
/// Only release pages of this repository are ever opened.
const RELEASES_PREFIX: &str = "https://github.com/joezhuo2/clawed/releases/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// Version without the leading `v`, e.g. `0.1.6`.
    pub version: String,
    pub url: String,
}

#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// `v1.2.3` or `1.2.3` to a comparable triple. Pre-release and build suffixes
/// (`1.2.3-rc.1`) are rejected so they are never offered as updates.
pub fn parse_version(s: &str) -> Option<(u64, u64, u64)> {
    let s = s.strip_prefix('v').unwrap_or(s);
    let mut parts = s.split('.');
    let v = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    parts.next().is_none().then_some(v)
}

pub fn is_newer(latest: &str, current: &str) -> bool {
    matches!((parse_version(latest), parse_version(current)), (Some(l), Some(c)) if l > c)
}

/// The release from a `releases/latest` body, if it is newer than `current`.
pub fn newer_release(body: &str, current: &str) -> Option<Release> {
    let raw: RawRelease = serde_json::from_str(body).ok()?;
    if raw.draft || raw.prerelease || !raw.html_url.starts_with(RELEASES_PREFIX) {
        return None;
    }
    let version = raw.tag_name.strip_prefix('v').unwrap_or(&raw.tag_name).to_string();
    is_newer(&version, current).then_some(Release { version, url: raw.html_url })
}

/// Blocking request; call from a blocking thread. `Ok(None)`: up to date.
pub fn check() -> Result<Option<Release>, String> {
    let mut resp = crate::oauth::agent()
        .get(LATEST_URL)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", concat!("clawed/", env!("CARGO_PKG_VERSION")))
        .call()
        .map_err(|e| e.to_string())?;
    let body = resp.body_mut().read_to_string().map_err(|e| e.to_string())?;
    Ok(newer_release(&body, env!("CARGO_PKG_VERSION")))
}

/// Opens a release page in the default browser.
pub fn open(url: &str) {
    if !url.starts_with(RELEASES_PREFIX) {
        return;
    }
    #[cfg(windows)]
    let cmd = std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn();
    #[cfg(target_os = "macos")]
    let cmd = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = std::process::Command::new("xdg-open").arg(url).spawn();
    if let Err(e) = cmd {
        log::warn!("open {url}: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions() {
        assert_eq!(parse_version("v0.1.5"), Some((0, 1, 5)));
        assert_eq!(parse_version("10.2.30"), Some((10, 2, 30)));
        assert_eq!(parse_version("0.1.5-rc.1"), None);
        assert_eq!(parse_version("0.1"), None);
        assert_eq!(parse_version("0.1.5.1"), None);
        assert!(is_newer("0.1.10", "0.1.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.1.5", "0.1.5"));
        assert!(!is_newer("0.1.4", "0.1.5"));
        assert!(!is_newer("garbage", "0.1.5"));
    }

    fn body(tag: &str, url: &str, prerelease: bool) -> String {
        serde_json::json!({ "tag_name": tag, "html_url": url, "draft": false, "prerelease": prerelease }).to_string()
    }

    #[test]
    fn release_selection() {
        let url = "https://github.com/joezhuo2/clawed/releases/tag/v0.1.6";
        assert_eq!(
            newer_release(&body("v0.1.6", url, false), "0.1.5"),
            Some(Release { version: "0.1.6".into(), url: url.into() })
        );
        assert_eq!(newer_release(&body("v0.1.5", url, false), "0.1.5"), None);
        assert_eq!(newer_release(&body("v0.1.6", url, true), "0.1.5"), None);
        assert_eq!(newer_release(&body("v0.1.6", "https://evil.example/x", false), "0.1.5"), None);
        assert_eq!(newer_release("{\"message\":\"Not Found\"}", "0.1.5"), None);
    }

    #[test]
    #[ignore = "network"]
    fn check_live() {
        println!("{:?}", check());
    }
}
