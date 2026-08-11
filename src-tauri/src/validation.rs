//! Generated-page validator (SRS §15).

use regex::Regex;

/// Runs cheap static checks against a generated HTML page and returns
/// "valid" or "failed". We only reject pages that clearly contain blocked
/// patterns — the full smoke-test in a headless WebView is deferred.
pub fn validate_page_html(html: &str) -> String {
    // Blocklist: remote scripts, network calls, dynamic imports of URLs.
    let patterns = [
        r#"<script[^>]+src\s*=\s*["']https?://"#,
        r#"\bfetch\s*\(\s*["']https?://"#,
        r#"\bXMLHttpRequest\b"#,
        r#"\bimport\s*\(\s*["']https?://"#,
        r#"\bnavigator\s*\.\s*sendBeacon\b"#,
    ];

    for p in patterns {
        if let Ok(re) = Regex::new(&format!("(?i){p}")) {
            if re.is_match(html) {
                tracing::warn!(pattern = p, "page validation failed");
                return "failed".into();
            }
        }
    }

    // Must contain a <title>.
    if !html.to_lowercase().contains("<title>") {
        return "failed".into();
    }

    "valid".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_remote_script() {
        let html = r#"<html><head><title>x</title><script src="https://evil.example/a.js"></script></head></html>"#;
        assert_eq!(validate_page_html(html), "failed");
    }

    #[test]
    fn rejects_fetch_to_remote() {
        let html = r#"<html><head><title>x</title></head><body><script>fetch("https://foo")</script></body></html>"#;
        assert_eq!(validate_page_html(html), "failed");
    }

    #[test]
    fn requires_title() {
        let html = r#"<html><head></head><body>hi</body></html>"#;
        assert_eq!(validate_page_html(html), "failed");
    }

    #[test]
    fn accepts_clean_page() {
        let html = r#"<html><head><title>Concept</title></head><body>
            <h1>Hello</h1>
            <script>console.log("local only");</script>
        </body></html>"#;
        assert_eq!(validate_page_html(html), "valid");
    }
}
