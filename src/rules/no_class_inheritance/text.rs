use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `class Foo extends Bar` patterns.
fn has_class_extends(line: &str) -> bool {
    let trimmed = line.trim();
    // Match `class <name> extends <name>`.
    if let Some(class_pos) = trimmed.find("class ") {
        let after_class = &trimmed[class_pos + 6..];
        // Check that `extends` appears after the class name.
        if after_class.contains(" extends ") {
            return true;
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') {
                continue;
            }
            if has_class_extends(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-class-inheritance".into(),
                    message: "Class inheritance via `extends` — prefer composition over inheritance.".into(),
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
    fn flags_class_extends() {
        assert_eq!(run("class Dog extends Animal {").len(), 1);
    }

    #[test]
    fn flags_export_class_extends() {
        assert_eq!(run("export class Foo extends Base {").len(), 1);
    }

    #[test]
    fn allows_class_without_extends() {
        assert!(run("class Foo {").is_empty());
    }

    #[test]
    fn allows_comment_with_extends() {
        assert!(run("// class Foo extends Bar").is_empty());
    }
}
