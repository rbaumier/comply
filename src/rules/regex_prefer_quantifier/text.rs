use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Tokenize regex pattern into elements (single chars or escape sequences like `\d`).
fn tokenize(pattern: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 1 < bytes.len() {
            tokens.push(&pattern[i..i + 2]);
            i += 2;
        } else if bytes[i] == b'[' {
            // Skip character class entirely
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b']' {
                if bytes[i] == b'\\' {
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if i < bytes.len() {
                i += 1; // skip ]
            }
            tokens.push(&pattern[start..i]);
        } else if bytes[i] == b'(' || bytes[i] == b')' || bytes[i] == b'|' {
            // Group/alternation markers — treat as non-repeatable
            tokens.push(&pattern[i..i + 1]);
            i += 1;
        } else if bytes[i] == b'{' {
            // Quantifier — skip to end
            let start = i;
            while i < bytes.len() && bytes[i] != b'}' {
                i += 1;
            }
            if i < bytes.len() {
                i += 1;
            }
            tokens.push(&pattern[start..i]);
        } else if bytes[i] == b'?' || bytes[i] == b'+' || bytes[i] == b'*' {
            tokens.push(&pattern[i..i + 1]);
            i += 1;
        } else {
            tokens.push(&pattern[i..i + 1]);
            i += 1;
        }
    }
    tokens
}

/// Check for 3+ consecutive identical tokens in a regex pattern.
fn has_repeated_tokens(pattern: &str) -> bool {
    let tokens = tokenize(pattern);
    let mut run = 1;
    for i in 1..tokens.len() {
        // Only compare non-structural tokens (skip quantifiers, groups, etc.)
        let prev = tokens[i - 1];
        let cur = tokens[i];
        if cur == prev
            && !matches!(
                cur,
                "(" | ")" | "|" | "?" | "+" | "*" | "^" | "$" | "."
            )
            && !cur.starts_with('{')
            && !cur.starts_with('[')
        {
            run += 1;
            if run >= 3 {
                return true;
            }
        } else {
            run = 1;
        }
    }
    false
}

/// Extract regex pattern from a line and check for repeated tokens.
fn has_prefer_quantifier(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' {
            if i > 0 {
                let prev = bytes[i - 1];
                if prev.is_ascii_alphanumeric() || prev == b')' || prev == b']' {
                    i += 1;
                    continue;
                }
            }
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == b'/' {
                    let pattern = &line[start..j];
                    if has_repeated_tokens(pattern) {
                        return true;
                    }
                    break;
                }
                if bytes[j] == b'\n' {
                    break;
                }
                j += 1;
            }
            i = j + 1;
            continue;
        }
        i += 1;
    }
    // Check RegExp constructor
    if let Some(pos) = line.find("RegExp(") {
        let rest = &line[pos + 7..];
        if let Some(q) = rest.find(|c| c == '"' || c == '\'') {
            let quote = rest.as_bytes()[q];
            let inner = &rest[q + 1..];
            if let Some(end) = inner.find(quote as char) {
                let pattern = &inner[..end];
                if has_repeated_tokens(pattern) {
                    return true;
                }
            }
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_prefer_quantifier(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "regex-prefer-quantifier".into(),
                    message: "Repeated identical pattern in regex — use a quantifier like `a{3}` or `\\d{4}`.".into(),
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
    fn flags_repeated_chars() {
        assert_eq!(run("const re = /aaa/;").len(), 1);
    }

    #[test]
    fn flags_repeated_escape() {
        assert_eq!(run(r#"const re = /\d\d\d\d/;"#).len(), 1);
    }

    #[test]
    fn allows_two_chars() {
        assert!(run("const re = /aa/;").is_empty());
    }

    #[test]
    fn allows_quantifier_already() {
        assert!(run("const re = /a{3}/;").is_empty());
    }

    #[test]
    fn flags_regexp_constructor() {
        assert_eq!(run(r#"const re = new RegExp("aaa");"#).len(), 1);
    }
}
