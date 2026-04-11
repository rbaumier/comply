use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Detect ): any { or ): any => or ): Promise<any>
fn has_return_type_any(line: &str) -> bool {
    let trimmed = line.trim();

    // ): any {  or  ): any =>
    if trimmed.contains("): any {") || trimmed.contains("): any =>") || trimmed.contains("): any;")
    {
        return true;
    }

    // ): Promise<any>
    if trimmed.contains("): Promise<any>") {
        return true;
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_return_type_any(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-return-type-any".into(),
                    message: "Function has explicit `: any` return type — use a specific type or `unknown`.".into(),
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
    fn flags_any_return_function() {
        assert_eq!(run("function foo(): any {").len(), 1);
    }

    #[test]
    fn flags_any_return_arrow() {
        assert_eq!(run("const foo = (): any => {};").len(), 1);
    }

    #[test]
    fn flags_promise_any_return() {
        assert_eq!(run("async function foo(): Promise<any> {").len(), 1);
    }

    #[test]
    fn allows_specific_return_type() {
        assert!(run("function foo(): string {").is_empty());
    }

    #[test]
    fn allows_unknown_return() {
        assert!(run("function foo(): unknown {").is_empty());
    }
}
