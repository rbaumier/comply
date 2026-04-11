use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects `new URL('./...', ` — a two-argument `new URL` whose first
/// argument is a string literal starting with `./`.
fn has_dot_slash_url(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find("new URL(") {
        let abs = start + pos;

        // Make sure `URL` is not part of a longer identifier.
        if abs > 0 {
            let _prev = line.as_bytes()[abs + 3]; // char before 'U' — but actually check before 'new'
            // abs points at 'n' of 'new'. Check char before it.
            if abs > 0 {
                let before_new = line.as_bytes()[abs - 1];
                if before_new.is_ascii_alphanumeric() || before_new == b'_' {
                    start = abs + 8;
                    continue;
                }
            }
        }

        let after_paren = abs + 8; // skip "new URL("
        let rest = &line[after_paren..];

        // Check for string literal starting with './'
        let trimmed = rest.trim_start();
        let has_dot_slash = if trimmed.starts_with("'./") || trimmed.starts_with("\"./") {
            // Make sure there's a second argument (a comma after the closing quote).
            let quote = trimmed.as_bytes()[0];
            // Find matching close quote
            let inner = &trimmed[1..];
            if let Some(close_pos) = find_unescaped_quote(inner, quote) {
                let after_string = &inner[close_pos + 1..].trim_start();
                after_string.starts_with(',')
            } else {
                false
            }
        } else if trimmed.starts_with("`./") {
            // Template literal — check for comma after closing backtick
            let inner = &trimmed[1..];
            if let Some(close_pos) = inner.find('`') {
                let after_string = &inner[close_pos + 1..].trim_start();
                after_string.starts_with(',')
            } else {
                false
            }
        } else {
            false
        };

        if has_dot_slash {
            return true;
        }

        start = after_paren;
    }
    false
}

/// Find the position of the first unescaped occurrence of `quote` in `s`.
fn find_unescaped_quote(s: &str, quote: u8) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == quote {
            return Some(i);
        }
        i += 1;
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if has_dot_slash_url(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "relative-url-style".into(),
                    message: "Remove the `./` prefix from the relative URL in `new URL()`."
                        .into(),
                    severity: Severity::Warning,
                });
            }
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
    fn flags_dot_slash_single_quotes() {
        assert_eq!(
            run("const url = new URL('./file.js', base);").len(),
            1
        );
    }

    #[test]
    fn flags_dot_slash_double_quotes() {
        assert_eq!(
            run(r#"const url = new URL("./file.js", base);"#).len(),
            1
        );
    }

    #[test]
    fn flags_dot_slash_template_literal() {
        assert_eq!(
            run("const url = new URL(`./file.js`, base);").len(),
            1
        );
    }

    #[test]
    fn allows_without_dot_slash() {
        assert!(run("const url = new URL('file.js', base);").is_empty());
    }

    #[test]
    fn allows_single_argument_url() {
        // Single argument URL — no base, so `./` might be meaningful
        assert!(run("const url = new URL('./file.js');").is_empty());
    }

    #[test]
    fn allows_absolute_url() {
        assert!(run("const url = new URL('https://example.com', base);").is_empty());
    }

    #[test]
    fn allows_comment() {
        assert!(run("// new URL('./file.js', base)").is_empty());
    }
}
