use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects quantified expressions at the start/end of lookaround assertions
/// that should only match a constant number of times.
/// Example: `(?=a+)` — the `+` in a lookahead is misleading since the
/// lookahead only checks if `a` is present, not how many times.
fn find_suboptimal_lookaround_quantifiers(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 4 < len {
        // Match (?= (?! (?<= (?<!
        if bytes[i] == b'(' && bytes[i + 1] == b'?' {
            let is_lookahead = bytes[i + 2] == b'=' || bytes[i + 2] == b'!';
            let is_lookbehind = bytes[i + 2] == b'<'
                && i + 3 < len
                && (bytes[i + 3] == b'=' || bytes[i + 3] == b'!');

            if is_lookahead || is_lookbehind {
                let content_start = if is_lookbehind { i + 4 } else { i + 3 };
                if let Some(close) = find_close_paren(bytes, i) {
                    let content = &line[content_start..close];
                    let cbytes = content.as_bytes();

                    if is_lookahead {
                        // Check end of content for quantifier
                        let clen = cbytes.len();
                        if clen > 0 && is_quantifier(cbytes[clen - 1]) {
                            hits.push(i);
                        }
                    } else {
                        // Lookbehind: check start of content for quantifier on first element
                        if cbytes.len() > 1 && is_quantifier(cbytes[1]) {
                            hits.push(i);
                        }
                    }
                }
            }
        }
        i += 1;
    }
    hits
}

fn is_quantifier(b: u8) -> bool {
    b == b'+' || b == b'*'
}

fn find_close_paren(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth = 1;
    let mut j = start + 1;
    while j < bytes.len() {
        match bytes[j] {
            b'\\' => j += 1,
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_suboptimal_lookaround_quantifiers(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-optimal-lookaround-quantifier".into(),
                    message: "Quantifier at the edge of a lookaround is misleading \u{2014} it should match a constant number of times.".into(),
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
    fn flags_quantifier_in_lookahead() {
        assert_eq!(run(r#"const re = /(?=a+)/;"#).len(), 1);
    }

    #[test]
    fn allows_no_quantifier_in_lookahead() {
        assert!(run(r#"const re = /(?=a)/;"#).is_empty());
    }

    #[test]
    fn flags_star_in_negative_lookahead() {
        assert_eq!(run(r#"const re = /(?!a*)/;"#).len(), 1);
    }
}
