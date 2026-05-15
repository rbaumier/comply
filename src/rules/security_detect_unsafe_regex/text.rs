//! security-detect-unsafe-regex text backend — pattern-level heuristic.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// True if `pattern` contains the classic "evil regex" shape
/// `(...+)+`, `(...*)*`, `(...+)*`, `(...*)+` — nested quantifiers on
/// a group, which makes the matcher backtrack exponentially.
fn has_nested_quantifier_group(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Skip escaped chars.
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'(' {
            // Find matching ')' (depth-aware, skipping escapes).
            let mut depth = 1usize;
            let mut j = i + 1;
            let mut inner_has_quantifier = false;
            while j < bytes.len() && depth > 0 {
                match bytes[j] {
                    b'\\' => {
                        j += 2;
                        continue;
                    }
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    b'+' | b'*' => {
                        inner_has_quantifier = true;
                    }
                    _ => {}
                }
                j += 1;
            }
            // Now `j` is the matching ')'. Check the next char.
            if j < bytes.len() && depth == 0 && inner_has_quantifier {
                let next = bytes.get(j + 1).copied();
                if matches!(next, Some(b'+') | Some(b'*')) {
                    return true;
                }
            }
            i = j + 1;
            continue;
        }
        i += 1;
    }
    false
}

fn line_col_at(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, c) in source.char_indices() {
        if i >= offset {
            break;
        }
        if c == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        let src = ctx.source;
        // Walk every regex literal `/<pattern>/<flags>`. A regex
        // literal starts with `/`, isn't `//` (line comment), isn't
        // `/*` (block comment), isn't division.
        let bytes = src.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'/'
                && bytes.get(i + 1).copied().is_some_and(|c| c != b'/' && c != b'*')
            {
                // Find the closing `/` (skip escapes and char classes).
                let mut j = i + 1;
                let mut in_class = false;
                while j < bytes.len() {
                    match bytes[j] {
                        b'\\' => {
                            j += 2;
                            continue;
                        }
                        b'[' => in_class = true,
                        b']' => in_class = false,
                        b'/' if !in_class => break,
                        b'\n' => break,
                        _ => {}
                    }
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'/' && j > i + 1 {
                    let pattern = &src[i + 1..j];
                    if has_nested_quantifier_group(pattern) {
                        let (line, column) = line_col_at(src, i);
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "Regex has nested quantifiers — vulnerable to \
                                      catastrophic backtracking (ReDoS) on adversarial \
                                      input."
                                .into(),
                            severity: Severity::Error,
                            span: None,
                        });
                    }
                    i = j + 1;
                    continue;
                }
            }
            i += 1;
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(src: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), src))
    }

    #[test]
    fn flags_evil_regex_plus_plus() {
        let src = r"const r = /(a+)+/;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_evil_regex_star_plus() {
        let src = r"const r = /(.*)+$/;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_safe_regex() {
        let src = r"const r = /^[a-z]+@[a-z]+\.[a-z]{2,4}$/;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_simple_grouping() {
        let src = r"const r = /^(foo|bar)$/;";
        assert!(run(src).is_empty());
    }
}
