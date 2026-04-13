use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects confusing quantifiers where the minimum is non-zero but the
/// quantified element can match the empty string.
/// Example: `(?:a?)+` — the `+` requires at least 1 match, but `a?` can be empty.
fn find_confusing_quantifiers(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'(' {
            let group_start = i;
            let mut depth = 1;
            let mut j = i + 1;
            let mut inner_has_optional = false;

            while j < len && depth > 0 {
                match bytes[j] {
                    b'\\' => j += 1,
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    b'?' if depth == 1 && j > 0 && bytes[j - 1] != b'(' && bytes[j - 1] != b'\\' => {
                        inner_has_optional = true;
                    }
                    b'*' if depth == 1 => {
                        inner_has_optional = true;
                    }
                    _ => {}
                }
                j += 1;
            }

            // Group ended at j, check if followed by `+` or `{n,}` with n > 0
            if depth == 0 && inner_has_optional && j + 1 < len {
                let next = bytes[j + 1];
                if next == b'+' {
                    hits.push(group_start);
                } else if next == b'{' {
                    // Check for {n,} where n > 0
                    if let Some(min) = parse_min_quantifier(&line[j + 1..])
                        && min > 0 {
                            hits.push(group_start);
                        }
                }
            }
        }
        i += 1;
    }
    hits
}

fn parse_min_quantifier(s: &str) -> Option<usize> {
    if !s.starts_with('{') {
        return None;
    }
    let inner = &s[1..];
    let end = inner.find('}')?;
    let content = &inner[..end];
    let parts: Vec<&str> = content.split(',').collect();
    parts.first()?.parse().ok()
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_confusing_quantifiers(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-confusing-quantifier".into(),
                    message: "Confusing quantifier \u{2014} minimum is non-zero but the element can match empty string.".into(),
                    severity: Severity::Warning,
                    span: None,
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
    fn flags_optional_in_plus_group() {
        assert_eq!(run(r#"const re = /(?:a?)+/;"#).len(), 1);
    }

    #[test]
    fn allows_required_in_plus_group() {
        assert!(run(r#"const re = /(?:a)+/;"#).is_empty());
    }

    #[test]
    fn flags_star_in_plus_group() {
        assert_eq!(run(r#"const re = /(?:a*)+/;"#).len(), 1);
    }
}
