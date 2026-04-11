use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Returns true if the text between braces is empty or contains only
/// whitespace and comments.
fn is_empty_or_comments_only(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return true;
    }
    // Check if only single-line comments and whitespace remain.
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        return false;
    }
    true
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let src = ctx.source;

        // Search for `catch` followed by optional `(...)` then `{ }`.
        let mut pos = 0;
        while pos < src.len() {
            let Some(catch_pos) = src[pos..].find("catch") else {
                break;
            };
            let catch_abs = pos + catch_pos;

            // Verify "catch" is not part of a longer identifier.
            let before_ok = catch_abs == 0
                || !src.as_bytes()[catch_abs - 1].is_ascii_alphanumeric()
                    && src.as_bytes()[catch_abs - 1] != b'_';
            let after_pos = catch_abs + 5;
            let after_ok = after_pos >= src.len()
                || !src.as_bytes()[after_pos].is_ascii_alphanumeric()
                    && src.as_bytes()[after_pos] != b'_';

            if !before_ok || !after_ok {
                pos = after_pos;
                continue;
            }

            // Skip whitespace after "catch".
            let rest = src[after_pos..].trim_start();
            let rest_offset = src.len() - rest.len();

            // Skip optional `(...)`.
            let body_start;
            if rest.starts_with('(') {
                if let Some(close_paren) = rest.find(')') {
                    let after_paren = rest[close_paren + 1..].trim_start();
                    let after_paren_offset = src.len() - after_paren.len();
                    if after_paren.starts_with('{') {
                        body_start = after_paren_offset + 1;
                    } else {
                        pos = after_pos;
                        continue;
                    }
                } else {
                    pos = after_pos;
                    continue;
                }
            } else if rest.starts_with('{') {
                body_start = rest_offset + 1;
            } else {
                pos = after_pos;
                continue;
            }

            // Find the matching closing brace.
            if let Some(close_brace) = src[body_start..].find('}') {
                let body = &src[body_start..body_start + close_brace];
                if is_empty_or_comments_only(body) {
                    let line = src[..catch_abs].matches('\n').count() + 1;
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line,
                        column: 1,
                        rule_id: "no-ignored-exceptions".into(),
                        message: "Empty `catch` block silently swallows the exception — log or re-throw it.".into(),
                        severity: Severity::Error,
                    });
                }
            }

            pos = after_pos;
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source))
    }

    #[test]
    fn flags_empty_catch() {
        let src = r#"
try { doSomething(); } catch (e) {}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_catch_with_only_whitespace() {
        let src = r#"
try {
  doSomething();
} catch (e) {

}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_catch_with_only_comments() {
        let src = r#"
try {
  doSomething();
} catch (e) {
  // intentionally empty
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_catch_with_handler() {
        let src = r#"
try { doSomething(); } catch (e) { console.error(e); }
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_catch_with_rethrow() {
        let src = r#"
try { doSomething(); } catch (e) { throw e; }
"#;
        assert!(run(src).is_empty());
    }
}
