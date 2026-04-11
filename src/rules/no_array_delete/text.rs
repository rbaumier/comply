use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detects `delete identifier[` pattern.
fn has_array_delete(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find("delete ") {
        let abs = start + pos + 7; // skip past "delete "
        let rest = &line[abs..];
        let rest_trimmed = rest.trim_start();
        // Expect an identifier followed by `[`.
        if let Some(bracket) = rest_trimmed.find('[') {
            let before_bracket = &rest_trimmed[..bracket];
            // The part before `[` should be a valid identifier (letters, digits, underscores, dots).
            let is_ident = !before_bracket.is_empty()
                && before_bracket
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '$');
            if is_ident {
                return true;
            }
        }
        start = abs;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_array_delete(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-array-delete".into(),
                    message:
                        "`delete arr[i]` creates a sparse hole — use `arr.splice(i, 1)` instead."
                            .into(),
                    severity: Severity::Error,
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
    fn flags_delete_array_element() {
        assert_eq!(run("delete arr[0];").len(), 1);
    }

    #[test]
    fn flags_delete_with_variable_index() {
        assert_eq!(run("delete items[idx];").len(), 1);
    }

    #[test]
    fn allows_delete_object_property() {
        // `delete obj.prop` has no bracket — should not flag.
        assert!(run("delete obj.prop;").is_empty());
    }

    #[test]
    fn ignores_non_delete_lines() {
        assert!(run("const x = arr[0];").is_empty());
    }
}
