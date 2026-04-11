use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects string disjunctions of single characters in `v`-flag character classes.
/// Example: `/[\q{a|b}]/v` can be simplified to `/[ab]/v`.
fn find_useless_string_literals(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'/' {
            if i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b')') {
                i += 1;
                continue;
            }
            let start = i + 1;
            let mut j = start;
            while j < len {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == b'/' {
                    let flag_start = j + 1;
                    let mut flag_end = flag_start;
                    while flag_end < len && bytes[flag_end].is_ascii_alphabetic() {
                        flag_end += 1;
                    }
                    let flags = &line[flag_start..flag_end];
                    if flags.contains('v') {
                        let pattern = &line[start..j];
                        if has_single_char_string_disjunction(pattern) {
                            hits.push(i);
                        }
                    }
                    i = flag_end;
                    break;
                }
                j += 1;
            }
        }
        i += 1;
    }
    hits
}

fn has_single_char_string_disjunction(pattern: &str) -> bool {
    // Look for \q{X|Y} where X and Y are single characters
    let mut search_from = 0;
    while let Some(pos) = pattern[search_from..].find("\\q{") {
        let start = search_from + pos + 3;
        if let Some(end) = pattern[start..].find('}') {
            let content = &pattern[start..start + end];
            let parts: Vec<&str> = content.split('|').collect();
            if parts.len() >= 2 && parts.iter().all(|p| p.chars().count() == 1) {
                return true;
            }
        }
        search_from = start;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_useless_string_literals(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-useless-string-literal".into(),
                    message: "String disjunction of single characters can be simplified to a character class.".into(),
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
    fn flags_single_char_disjunction() {
        assert_eq!(run(r#"const re = /[\q{a|b}]/v;"#).len(), 1);
    }

    #[test]
    fn allows_multi_char_string() {
        assert!(run(r#"const re = /[\q{ab|cd}]/v;"#).is_empty());
    }

    #[test]
    fn allows_non_v_flag() {
        assert!(run(r#"const re = /foo/g;"#).is_empty());
    }
}
