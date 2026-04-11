use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects trivially nested lookaround assertions of the same kind.
/// Example: `(?=(?=a)b)` — nested `(?=a)` inside `(?=...)` is trivially nested.
fn find_trivially_nested(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 4 < len {
        if bytes[i] == b'(' && bytes[i + 1] == b'?' {
            let outer_kind = get_lookaround_kind(bytes, i);
            if let Some(kind) = outer_kind {
                // Scan inside for the same kind of assertion
                let content_start = i + kind.len() + 2; // skip `(?` + kind chars
                let mut j = content_start;
                let mut depth = 1;
                while j + 3 < len && depth > 0 {
                    if bytes[j] == b'\\' {
                        j += 2;
                        continue;
                    }
                    if bytes[j] == b'(' && bytes[j + 1] == b'?' {
                        if let Some(inner_kind) = get_lookaround_kind(bytes, j)
                            && inner_kind == kind {
                                hits.push(i);
                                break;
                            }
                        depth += 1;
                    } else if bytes[j] == b'(' {
                        depth += 1;
                    } else if bytes[j] == b')' {
                        depth -= 1;
                    }
                    j += 1;
                }
            }
        }
        i += 1;
    }
    hits
}

fn get_lookaround_kind(bytes: &[u8], pos: usize) -> Option<&'static str> {
    if pos + 3 > bytes.len() || bytes[pos] != b'(' || bytes[pos + 1] != b'?' {
        return None;
    }
    match bytes[pos + 2] {
        b'=' => Some("="),
        b'!' => Some("!"),
        b'<' if pos + 4 <= bytes.len() => match bytes[pos + 3] {
            b'=' => Some("<="),
            b'!' => Some("<!"),
            _ => None,
        },
        _ => None,
    }
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_trivially_nested(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-trivially-nested-assertion".into(),
                    message: "Trivially nested lookaround assertion \u{2014} merge with parent or simplify.".into(),
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
    fn flags_nested_same_lookahead() {
        assert_eq!(run(r#"const re = /(?=(?=a)b)/;"#).len(), 1);
    }

    #[test]
    fn allows_different_lookaround_kinds() {
        assert!(run(r#"const re = /(?=(?!a)b)/;"#).is_empty());
    }

    #[test]
    fn flags_nested_lookbehind() {
        assert_eq!(run(r#"const re = /(?<=(?<=a)b)/;"#).len(), 1);
    }
}
