//! html-prefer-https — scans HTML/Vue text for insecure `http://` URLs in
//! `href`, `src`, or `action` attributes. Localhost URLs are allowed since
//! local development typically runs over plain HTTP.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const ATTRS: &[&str] = &["href", "src", "action"];
const ALLOWED_HOSTS: &[&str] = &["http://localhost", "http://127.0.0.1"];

#[derive(Debug)]
pub struct Check;

/// Returns true when `prefix` (the line content up to a candidate attribute
/// match) ends inside an unclosed attribute-value quote. In that case the
/// candidate is HTML embedded in another attribute's string value (e.g. an
/// `attribution` prop carrying `<a href='http://...'>` markup), not a real
/// resource-loading attribute, so it must not be flagged.
///
/// A quote only opens a value when it directly follows `=` (whitespace
/// allowed), so apostrophes or quotes in text content (e.g. `Don't`) do not
/// corrupt the state and suppress later real attributes on the same line.
fn inside_attribute_value(prefix: &str) -> bool {
    let mut open_quote: Option<char> = None;
    let mut prev_significant: Option<char> = None;
    for ch in prefix.chars() {
        match open_quote {
            Some(q) if ch == q => open_quote = None,
            Some(_) => {}
            None if (ch == '"' || ch == '\'') && prev_significant == Some('=') => {
                open_quote = Some(ch);
            }
            None => {}
        }
        if !ch.is_whitespace() {
            prev_significant = Some(ch);
        }
    }
    open_quote.is_some()
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["http://"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (i, line) in ctx.source.lines().enumerate() {
            for attr in ATTRS {
                let needle_dq = format!("{attr}=\"http://");
                let needle_sq = format!("{attr}='http://");
                for needle in [&needle_dq, &needle_sq] {
                    let mut search_from = 0;
                    while let Some(rel) = line[search_from..].find(needle.as_str()) {
                        let match_start = search_from + rel;
                        let url_start = match_start + attr.len() + 2; // attr + `="` or `='`
                        let remainder = &line[url_start..];
                        let in_string_value = inside_attribute_value(&line[..match_start]);
                        if !in_string_value
                            && !ALLOWED_HOSTS.iter().any(|h| remainder.starts_with(h))
                        {
                            diagnostics.push(Diagnostic {
                                path: std::sync::Arc::clone(&ctx.path_arc),
                                line: i + 1,
                                column: match_start + 1,
                                rule_id: super::META.id.into(),
                                message: format!(
                                    "`{attr}` uses insecure `http://`. Use https:// instead."
                                ),
                                severity: Severity::Error,
                                span: None,
                            });
                        }
                        search_from = match_start + needle.len();
                    }
                }
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("Comp.vue"), src))
    }

    #[test]
    fn flags_http_href() {
        let src = "<template><a href=\"http://example.com\">x</a></template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_http_src() {
        let src = "<template><img src=\"http://cdn.example.com/a.png\" /></template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_http_action() {
        let src = "<template><form action=\"http://example.com/submit\"></form></template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_https() {
        let src = "<template><a href=\"https://example.com\">x</a></template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_localhost() {
        let src = "<template><a href=\"http://localhost:3000\">x</a></template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_loopback_ip() {
        let src = "<template><img src=\"http://127.0.0.1:8080/x.png\" /></template>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_single_quoted_attr() {
        let src = "<template><a href='http://example.com'>x</a></template>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_http_inside_attribution_string_value() {
        let src = "<l-tile-layer attribution=\"Map tiles by <a href='http://stamen.com'>Stamen Design</a>, under <a href='http://creativecommons.org/licenses/by/3.0'>CC BY 3.0</a>.\" />";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_real_href_after_closed_embedded_string_prop() {
        // First `href=` is inside the (closed) `data-html` value; the trailing
        // `href=` is a real attribute and must still be flagged.
        let src =
            "<a data-html=\"<a href='http://x'></a>\" href=\"http://example.com\">x</a>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn apostrophe_in_text_does_not_suppress_real_attr() {
        // A stray apostrophe in text content must not open a value and hide a
        // later real `http://` href.
        let src = "<span>Don't</span> <a href=\"http://example.com\">x</a>";
        assert_eq!(run(src).len(), 1);
    }
}
