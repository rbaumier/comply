use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const BUILTINS: &[&str] = &[
    "Array", "Object", "String", "Map", "Set", "Promise", "JSON", "Math",
    "undefined", "NaN", "Infinity",
];

/// Check if a line overrides a built-in global via `const X =`, `let X =`, or `var X =`.
fn find_override(line: &str) -> Option<&'static str> {
    let trimmed = line.trim();

    // Skip comments.
    if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
        return None;
    }

    for &builtin in BUILTINS {
        // Patterns: `const Array =`, `let Array =`, `var Array =`
        // Also handle `export const Array =` etc.
        let patterns = [
            format!("const {} =", builtin),
            format!("const {}=", builtin),
            format!("let {} =", builtin),
            format!("let {}=", builtin),
            format!("var {} =", builtin),
            format!("var {}=", builtin),
        ];
        for pat in &patterns {
            if trimmed.contains(pat.as_str()) {
                return Some(builtin);
            }
        }
    }

    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(builtin) = find_override(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-built-in-override".into(),
                    message: format!(
                        "Overriding built-in `{}` — rename this variable.",
                        builtin
                    ),
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
    fn flags_const_array_override() {
        let d = run("const Array = [];");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-built-in-override");
        assert!(d[0].message.contains("Array"));
    }

    #[test]
    fn flags_let_object_override() {
        let d = run("let Object = {};");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Object"));
    }

    #[test]
    fn flags_var_string_override() {
        let d = run("var String = 'hello';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("String"));
    }

    #[test]
    fn flags_promise_override() {
        let d = run("const Promise = null;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_undefined_override() {
        let d = run("const undefined = 42;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("undefined"));
    }

    #[test]
    fn allows_normal_variables() {
        assert!(run("const myArray = [];").is_empty());
        assert!(run("let objectMapper = {};").is_empty());
        assert!(run("const str = String(42);").is_empty());
    }

    #[test]
    fn allows_usage_not_assignment() {
        assert!(run("const x = Array.from([1, 2, 3]);").is_empty());
        assert!(run("const m = new Map();").is_empty());
    }
}
