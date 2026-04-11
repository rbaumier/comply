use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect `new ClassName(` at the start of a statement (not assigned).
/// A standalone `new` is one where `new` is the first significant token on the line,
/// i.e. the trimmed line starts with `new ` and there is no `=` before `new`.
fn is_standalone_new(line: &str) -> bool {
    let trimmed = line.trim();
    // Line starts with `new ` — clearly unassigned
    if trimmed.starts_with("new ") {
        return true;
    }
    // Also catch: `  new Foo();` with only semicolons/whitespace
    // But NOT: `const x = new Foo();` or `return new Foo();` or `throw new Foo();`
    if let Some(pos) = trimmed.find("new ") {
        let before = trimmed[..pos].trim_end();
        // If anything before `new` is an assignment or keyword, it's not standalone
        if before.is_empty() {
            return true;
        }
        // Not standalone if preceded by `=`, `return`, `throw`, `yield`, `(`, `,`, `:`
        if before.ends_with('=')
            || before.ends_with("return")
            || before.ends_with("throw")
            || before.ends_with("yield")
            || before.ends_with('(')
            || before.ends_with(',')
            || before.ends_with(':')
            || before.ends_with("=>")
        {
            return false;
        }
        // If only `export default` or similar, still not standalone in most cases
        return false;
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if is_standalone_new(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-constructor-side-effects".into(),
                    message:
                        "`new X()` without assignment — constructors should not be called for side effects."
                            .into(),
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
    fn flags_standalone_new() {
        assert_eq!(run("new MyService();").len(), 1);
    }

    #[test]
    fn flags_standalone_new_indented() {
        assert_eq!(run("  new MyService();").len(), 1);
    }

    #[test]
    fn allows_assigned_new() {
        assert!(run("const svc = new MyService();").is_empty());
    }

    #[test]
    fn allows_returned_new() {
        assert!(run("return new MyService();").is_empty());
    }

    #[test]
    fn allows_thrown_new() {
        assert!(run("throw new Error('fail');").is_empty());
    }
}
