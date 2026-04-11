use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Match lines containing `async constructor(` with optional whitespace.
fn has_async_constructor(line: &str) -> bool {
    let trimmed = line.trim();
    // Match: `async constructor(` with any whitespace between tokens
    if let Some(pos) = trimmed.find("async") {
        let after = trimmed[pos + 5..].trim_start();
        if let Some(rest) = after.strip_prefix("constructor") {
            return rest.trim_start().starts_with('(');
        }
    }
    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if has_async_constructor(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-async-constructor".into(),
                    message:
                        "Constructors cannot be `async` — use a static async factory method instead."
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
    fn flags_async_constructor() {
        assert_eq!(run("  async constructor() {").len(), 1);
    }

    #[test]
    fn flags_async_constructor_with_params() {
        assert_eq!(run("  async constructor(name: string) {").len(), 1);
    }

    #[test]
    fn allows_regular_constructor() {
        assert!(run("  constructor() {").is_empty());
    }

    #[test]
    fn allows_async_method() {
        assert!(run("  async initialize() {").is_empty());
    }
}
