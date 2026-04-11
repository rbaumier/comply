use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const PURE_METHODS: &[&str] = &[
    ".map(",
    ".filter(",
    ".slice(",
    ".concat(",
    ".trim()",
    ".replace(",
    ".toUpperCase()",
    ".toLowerCase()",
    ".split(",
    ".join(",
];

/// Returns true if the line is a standalone call to a pure method — not
/// assigned, returned, or used as an argument.
fn is_ignored_pure_call(line: &str) -> Option<&'static str> {
    let trimmed = line.trim();

    // Must contain a pure method call.
    let method = PURE_METHODS.iter().find(|&&m| trimmed.contains(m))?;

    // If the line starts with assignment, return, yield, or is inside a
    // larger expression, it's not ignored.
    if trimmed.starts_with("const ")
        || trimmed.starts_with("let ")
        || trimmed.starts_with("var ")
        || trimmed.starts_with("return ")
        || trimmed.starts_with("return(")
        || trimmed.starts_with("yield ")
        || trimmed.contains(" = ") || trimmed.contains(" =\t")
        || trimmed.starts_with("//")
        || trimmed.starts_with('*')
        || trimmed.starts_with("/*")
        || trimmed.starts_with("export ")
    {
        return None;
    }

    Some(method)
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(method) = is_ignored_pure_call(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-ignored-return".into(),
                    message: format!(
                        "Return value of `{}` is ignored — the call has no side effect.",
                        method.trim_end_matches('(').trim_end_matches(')')
                    ),
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
    fn flags_standalone_map() {
        let d = run("  arr.map(x => x + 1);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains(".map"));
    }

    #[test]
    fn flags_standalone_filter() {
        assert_eq!(run("items.filter(Boolean);").len(), 1);
    }

    #[test]
    fn allows_assigned_map() {
        assert!(run("const doubled = arr.map(x => x * 2);").is_empty());
    }

    #[test]
    fn allows_returned_map() {
        assert!(run("return arr.map(x => x * 2);").is_empty());
    }

    #[test]
    fn flags_standalone_trim() {
        assert_eq!(run("  name.trim();").len(), 1);
    }
}
