//! html-prefer-https — scans HTML/Vue text for insecure `http://` URLs in
//! `href`, `src`, or `action` attributes. Localhost URLs are allowed since
//! local development typically runs over plain HTTP.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

const ATTRS: &[&str] = &["href", "src", "action"];
const ALLOWED_HOSTS: &[&str] = &["http://localhost", "http://127.0.0.1"];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
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
                        if !ALLOWED_HOSTS.iter().any(|h| remainder.starts_with(h)) {
                            diagnostics.push(Diagnostic {
                                path: ctx.path.to_path_buf(),
                                line: i + 1,
                                column: match_start + 1,
                                rule_id: super::META.id.into(),
                                message: format!(
                                    "`{attr}` uses insecure `http://`. Use https:// instead."
                                ),
                                severity: Severity::Warning,
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
}
