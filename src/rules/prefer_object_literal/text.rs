use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

fn has_new_object(line: &str) -> bool {
    line.contains("new Object(")
}

fn has_object_create_null(line: &str) -> bool {
    let mut start = 0;
    while let Some(pos) = line[start..].find("Object.create(") {
        let abs = start + pos + 14; // skip past "Object.create("
        let rest = &line[abs..];
        let trimmed = rest.trim_start();
        if trimmed.starts_with("null") {
            return true;
        }
        start = abs;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_new_object(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-object-literal".into(),
                    message: "Use `{}` instead of `new Object()`.".into(),
                    severity: Severity::Warning,
                });
            } else if has_object_create_null(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-object-literal".into(),
                    message: "Prefer an object literal over `Object.create(null)`.".into(),
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
    fn flags_new_object() {
        let d = run("const obj = new Object();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("new Object()"));
    }

    #[test]
    fn flags_object_create_null() {
        let d = run("const obj = Object.create(null);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Object.create(null)"));
    }

    #[test]
    fn allows_object_literal() {
        assert!(run("const obj = {};").is_empty());
    }

    #[test]
    fn allows_object_create_with_prototype() {
        assert!(run("const obj = Object.create(proto);").is_empty());
    }
}
