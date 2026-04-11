use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const SHADOWED_GLOBALS: &[&str] = &[
    "console",
    "window",
    "document",
    "process",
    "global",
    "globalThis",
    "setTimeout",
    "setInterval",
];

/// Check if a line declares a local variable that shadows a global.
/// Matches patterns like `const console = ...`, `let window = ...`, `var document = ...`.
fn shadows_global(line: &str) -> Option<&'static str> {
    let trimmed = line.trim();
    let rest = trimmed
        .strip_prefix("const ")
        .or_else(|| trimmed.strip_prefix("let "))
        .or_else(|| trimmed.strip_prefix("var "))?;
    let rest = rest.trim_start();
    for &g in SHADOWED_GLOBALS {
        if rest.starts_with(g) {
            let after = &rest[g.len()..];
            // Must be followed by whitespace, `=`, `:`, or `;` — not part of a longer ident
            if after.starts_with(|c: char| c == ' ' || c == '=' || c == ':' || c == ';') {
                return Some(g);
            }
        }
    }
    None
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(global_name) = shadows_global(line) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-globals-shadowing".into(),
                    message: format!(
                        "Local variable shadows global `{global_name}` — rename to avoid confusion."
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
    fn flags_const_console() {
        assert_eq!(run("const console = {};").len(), 1);
    }

    #[test]
    fn flags_let_window() {
        assert_eq!(run("let window = {};").len(), 1);
    }

    #[test]
    fn flags_var_document() {
        assert_eq!(run("var document = fake;").len(), 1);
    }

    #[test]
    fn allows_different_name() {
        assert!(run("const myConsole = {};").is_empty());
    }

    #[test]
    fn allows_console_usage() {
        assert!(run("console.log('hello');").is_empty());
    }
}
