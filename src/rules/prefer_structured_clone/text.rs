use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if line.contains("JSON.parse(JSON.stringify(") {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-structured-clone".into(),
                    message: "Prefer `structuredClone(…)` over `JSON.parse(JSON.stringify(…))` to create a deep clone.".into(),
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
    fn flags_json_parse_stringify() {
        let d = run("const copy = JSON.parse(JSON.stringify(obj));");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("structuredClone"));
    }

    #[test]
    fn flags_nested_expression() {
        let d = run("return JSON.parse(JSON.stringify(this.state));");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_structured_clone() {
        assert!(run("const copy = structuredClone(obj);").is_empty());
    }

    #[test]
    fn allows_json_parse_alone() {
        assert!(run("const data = JSON.parse(text);").is_empty());
    }

    #[test]
    fn allows_json_stringify_alone() {
        assert!(run("const text = JSON.stringify(obj);").is_empty());
    }
}
