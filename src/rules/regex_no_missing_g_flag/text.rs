use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Methods that require the `g` flag on their regex argument.
const G_REQUIRED_METHODS: &[&str] = &[".matchAll(", ".replaceAll("];

/// Detects regex passed to methods like `matchAll` / `replaceAll` without the `g` flag.
fn find_missing_g_flag(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();

    for method in G_REQUIRED_METHODS {
        let mut search_from = 0;
        while let Some(pos) = line[search_from..].find(method) {
            let abs_pos = search_from + pos;
            let after_paren = abs_pos + method.len();
            // Check if the argument starts with `/` (regex literal)
            let rest = &line[after_paren..];
            let trimmed = rest.trim_start();
            if trimmed.starts_with('/') {
                // Find the closing `/flags`
                let regex_start = after_paren + (rest.len() - trimmed.len()) + 1;
                if let Some(flags) = extract_flags_from_regex(line, regex_start)
                    && !flags.contains('g') {
                        hits.push(abs_pos);
                    }
            }
            search_from = abs_pos + method.len();
        }
    }
    hits
}

fn extract_flags_from_regex(line: &str, pattern_start: usize) -> Option<&str> {
    let bytes = line.as_bytes();
    let mut j = pattern_start;
    while j < bytes.len() {
        if bytes[j] == b'\\' {
            j += 2;
            continue;
        }
        if bytes[j] == b'/' {
            // Everything between closing `/` and next non-alpha is flags
            let flag_start = j + 1;
            let mut flag_end = flag_start;
            while flag_end < bytes.len() && bytes[flag_end].is_ascii_alphabetic() {
                flag_end += 1;
            }
            return Some(&line[flag_start..flag_end]);
        }
        j += 1;
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_missing_g_flag(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-missing-g-flag".into(),
                    message: "Regex passed to a method that requires the `g` flag but it is missing.".into(),
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
    fn flags_matchall_without_g() {
        assert_eq!(run(r#"str.matchAll(/foo/i);"#).len(), 1);
    }

    #[test]
    fn allows_matchall_with_g() {
        assert!(run(r#"str.matchAll(/foo/gi);"#).is_empty());
    }

    #[test]
    fn flags_replaceall_without_g() {
        assert_eq!(run(r#"str.replaceAll(/bar/, "baz");"#).len(), 1);
    }
}
