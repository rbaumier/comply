use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects JSDoc type annotations using bare `Function` or `function` instead
/// of a specific function signature.
/// Example: `@param {Function} cb` should be `@param {(x: string) => void} cb`.
fn find_bare_function_types(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let trimmed = line.trim();

    // Only check JSDoc comment lines
    if !trimmed.starts_with('*') && !trimmed.starts_with("/**") && !trimmed.starts_with("//") {
        return hits;
    }

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
                if type_content == "Function" || type_content == "function" {
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
            for col in find_bare_function_types(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: "jsdoc-reject-function-type".into(),
                    message: "JSDoc uses bare `Function` type \u{2014} provide a specific function signature instead.".into(),
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
    fn flags_bare_function() {
        assert_eq!(run(" * @param {Function} cb - callback").len(), 1);
    }

    #[test]
    fn flags_lowercase_function() {
        assert_eq!(run(" * @param {function} handler").len(), 1);
    }

    #[test]
    fn allows_specific_signature() {
        assert!(run(" * @param {(x: string) => void} cb").is_empty());
    }
}
