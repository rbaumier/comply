//! prefer-top-level-await text backend.
//!
//! Detects two common patterns at the top level of ESM files:
//!
//! 1. Async IIFE: `(async () => { ... })()`  /  `(async function() { ... })()`
//! 2. Async function + call: `async function main() { ... }` followed by `main()`
//!
//! CJS files (`.cjs`) are skipped since top-level await is ESM-only.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Returns `true` if the trimmed line starts with `(async ` followed by
/// `function` or something that looks like arrow params — detecting IIFE.
fn is_async_iife(trimmed: &str) -> bool {
    let Some(rest) = trimmed.strip_prefix("(async ") else { return false };
    let rest = rest.trim_start();
    rest.starts_with("function")
        || rest.starts_with('(')
        || rest.starts_with("()") // common: (async () => ...)()
        || rest.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
}

/// If the line is a top-level `async function <name>(`, returns the name.
fn extract_async_func_name(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("async ")?;
    let rest = rest.strip_prefix("function ")?;
    let rest = rest.trim_start();
    // Extract identifier: sequence of word chars
    let end = rest
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '$')
        .unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    let name = &rest[..end];
    // Must be followed by `(`
    let after = rest[end..].trim_start();
    if after.starts_with('(') {
        Some(name)
    } else {
        None
    }
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        // Skip CJS files — top-level await is ESM-only.
        let path_str = ctx.path.to_string_lossy();
        if path_str.ends_with(".cjs") {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        // Collect top-level async function names for pattern 2.
        let mut async_func_names: Vec<String> = Vec::new();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Pattern 1: async IIFE
            if is_async_iife(trimmed) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-top-level-await".into(),
                    message: "Prefer top-level await over an async IIFE.".into(),
                    severity: Severity::Warning,
                });
                continue;
            }

            // Pattern 2: collect top-level async function declarations
            // (must start at column 0 — no indentation)
            if let Some(name) = extract_async_func_name(line) {
                async_func_names.push(name.to_owned());
            }
        }

        // Pattern 2: check if any collected async function is called at top level
        for name in &async_func_names {
            let call_pattern = format!("{name}(");
            let then_pattern = format!("{name}().then(");
            for (idx, line) in lines.iter().enumerate() {
                let trimmed = line.trim();
                let is_top_level_call = trimmed.starts_with(&call_pattern)
                    || trimmed.starts_with(&then_pattern);
                if !is_top_level_call {
                    continue;
                }
                // Avoid flagging the declaration itself
                if trimmed.starts_with("async ") || trimmed.starts_with("export ") {
                    continue;
                }
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "prefer-top-level-await".into(),
                    message: format!(
                        "Prefer top-level await over calling async function `{name}()`."
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

    fn run_cjs(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.cjs"), source))
    }

    #[test]
    fn flags_async_arrow_iife() {
        let d = run("(async () => { await fetch('/api'); })();");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-top-level-await");
    }

    #[test]
    fn flags_async_function_iife() {
        let d = run("(async function() { await fetch('/api'); })();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_async_function_then_call() {
        let src = "async function main() {\n  await fetch('/api');\n}\nmain();";
        let d = run(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("main"));
    }

    #[test]
    fn flags_async_function_with_then() {
        let src = "async function bootstrap() {\n  await init();\n}\nbootstrap().then(() => {});";
        let d = run(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_regular_function() {
        let src = "function main() {\n  console.log('hello');\n}\nmain();";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_cjs_files() {
        let src = "(async () => { await fetch('/api'); })();";
        assert!(run_cjs(src).is_empty());
    }

    #[test]
    fn allows_top_level_await_directly() {
        assert!(run("const data = await fetch('/api');").is_empty());
    }
}
