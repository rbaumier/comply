use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects assertions inside optional (quantified with `?` or `*`) groups,
/// e.g. `(?:^a)?` or `(\b foo)*` — the assertion is effectively ignored.
fn find_optional_assertions(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'(' {
            let group_start = i;
            let mut depth = 1;
            let mut j = i + 1;
            let mut has_assertion = false;
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
                    b'^' | b'$' => {
                        if depth == 1 {
                            has_assertion = true;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            // Check for (?=...) or (?!...) or (?<=...) or (?<!...) inside group
            if !has_assertion {
                let inner_start = i + 1;
                let mut k = inner_start;
                while k + 2 < j {
                    if bytes[k] == b'(' && bytes[k + 1] == b'?' && (bytes[k + 2] == b'=' || bytes[k + 2] == b'!') {
                        has_assertion = true;
                        break;
                    }
                    k += 1;
                }
            }
            if depth == 0 && has_assertion && j + 1 < len {
                let next = bytes[j + 1];
                if next == b'?' || next == b'*' {
                    hits.push(group_start);
                }
            }
        }
        i += 1;
    }
    hits
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            for col in find_optional_assertions(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "regex-no-optional-assertion".into(),
                    message: "Assertion inside an optional group is effectively ignored.".into(),
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
    fn flags_assertion_in_optional_group() {
        assert_eq!(run(r#"const re = /(?:^foo)?bar/;"#).len(), 1);
    }

    #[test]
    fn allows_assertion_in_required_group() {
        assert!(run(r#"const re = /(?:^foo)bar/;"#).is_empty());
    }

    #[test]
    fn flags_assertion_in_star_group() {
        assert_eq!(run(r#"const re = /(?:^foo)*bar/;"#).len(), 1);
    }
}
