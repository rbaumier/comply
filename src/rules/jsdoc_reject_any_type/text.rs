use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects JSDoc type annotations using `*` or `any` which defeat the purpose
/// of type documentation.
/// Example: `@param {*} x` or `@returns {any}`.
fn find_any_types(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let trimmed = line.trim();

    // Only check JSDoc comment lines
    if !trimmed.starts_with('*') && !trimmed.starts_with("/**") && !trimmed.starts_with("//") {
        return hits;
    }

    // Look for `{*}` or `{any}`
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let start = i;
            let mut j = i + 1;
            while j < bytes.len() && bytes[j] != b'}' {
                j += 1;
            }
            if j < bytes.len() {
                let type_content = line[start + 1..j].trim();
                if type_content == "*" || type_content.eq_ignore_ascii_case("any") {
                    hits.push(start);
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
            for col in find_any_types(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "jsdoc-reject-any-type".into(),
                    message: "JSDoc uses `*` or `any` type \u{2014} provide a specific type instead.".into(),
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
    fn flags_star_type() {
        assert_eq!(run(" * @param {*} x - the value").len(), 1);
    }

    #[test]
    fn flags_any_type() {
        assert_eq!(run(" * @returns {any}").len(), 1);
    }

    #[test]
    fn allows_specific_type() {
        assert!(run(" * @param {string} x - the value").is_empty());
    }
}
