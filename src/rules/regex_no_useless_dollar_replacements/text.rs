use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects replacement strings that reference capturing groups which don't exist.
/// Example: `"foo".replace(/(a)/, "$2")` — `$2` doesn't exist, only `$1` does.
fn find_useless_dollar_replacements(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();

    // Look for .replace( or .replaceAll(
    for method in &[".replace(", ".replaceAll("] {
        let mut search_from = 0;
        while let Some(pos) = line[search_from..].find(method) {
            let abs_pos = search_from + pos;
            let after = abs_pos + method.len();
            let rest = &line[after..];

            // Try to find regex literal as first arg
            let trimmed = rest.trim_start();
            if trimmed.starts_with('/') {
                let regex_content_start = after + (rest.len() - trimmed.len()) + 1;
                if let Some((group_count, after_regex)) = count_groups_in_regex(line, regex_content_start) {
                    // Find the replacement string (second argument)
                    if let Some(max_ref) = find_max_dollar_ref(line, after_regex)
                        && max_ref > group_count {
                            hits.push(abs_pos);
                        }
                }
            }
            search_from = abs_pos + method.len();
        }
    }
    hits
}

fn count_groups_in_regex(line: &str, pattern_start: usize) -> Option<(usize, usize)> {
    let bytes = line.as_bytes();
    let mut groups = 0;
    let mut j = pattern_start;
    while j < bytes.len() {
        if bytes[j] == b'\\' {
            j += 2;
            continue;
        }
        if bytes[j] == b'(' && j + 1 < bytes.len() && bytes[j + 1] != b'?' {
            groups += 1;
        }
        if bytes[j] == b'/' {
            // Skip flags
            let mut k = j + 1;
            while k < bytes.len() && bytes[k].is_ascii_alphabetic() {
                k += 1;
            }
            return Some((groups, k));
        }
        j += 1;
    }
    None
}

fn find_max_dollar_ref(line: &str, from: usize) -> Option<usize> {
    let rest = &line[from..];
    // Find a string argument after comma
    let comma_pos = rest.find(',')?;
    let after_comma = rest[comma_pos + 1..].trim_start();
    let quote = match after_comma.as_bytes().first()? {
        b'"' | b'\'' | b'`' => after_comma.as_bytes()[0],
        _ => return None,
    };
    let inner = &after_comma[1..];
    let end = inner.find(quote as char)?;
    let replacement = &inner[..end];

    let mut max_ref = 0;
    let bytes = replacement.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
            let d = (bytes[i + 1] - b'0') as usize;
            if d > max_ref {
                max_ref = d;
            }
        }
        i += 1;
    }
    if max_ref > 0 { Some(max_ref) } else { None }
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_useless_dollar_replacements(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-useless-dollar-replacements".into(),
                    message: "Replacement string references a capturing group that does not exist in the regex.".into(),
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
    fn flags_nonexistent_group_ref() {
        assert_eq!(run(r#"str.replace(/(a)/, "$2");"#).len(), 1);
    }

    #[test]
    fn allows_valid_group_ref() {
        assert!(run(r#"str.replace(/(a)/, "$1");"#).is_empty());
    }

    #[test]
    fn flags_replaceall_nonexistent() {
        assert_eq!(run(r#"str.replaceAll(/(a)/g, "$3");"#).len(), 1);
    }
}
