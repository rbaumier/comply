//! ts-no-namespace backend — scan for `namespace` keyword (excluding
//! `declare namespace` in .d.ts files).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug)]
pub struct Check;

static NAMESPACE_DECL: LazyLock<Regex> = LazyLock::new(|| {
    // Match `namespace Foo {` or `export namespace Foo {`,
    // but NOT `declare namespace` (allowed in .d.ts), and not string matches.
    Regex::new(r"(?:^|\s)(?:export\s+)?namespace\s+\w+").unwrap()
});

static DECLARE_NAMESPACE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|\s)declare\s+(?:export\s+)?namespace\s").unwrap()
});

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let is_dts = ctx
            .path
            .to_str()
            .is_some_and(|p| p.ends_with(".d.ts") || p.ends_with(".d.mts") || p.ends_with(".d.cts"));
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            // Skip comments
            let trimmed = line.trim();
            if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
                continue;
            }
            if !NAMESPACE_DECL.is_match(line) {
                continue;
            }
            // Allow `declare namespace` in .d.ts files
            if is_dts && DECLARE_NAMESPACE.is_match(line) {
                continue;
            }
            // Also allow `declare namespace` in regular files (ambient declarations)
            if DECLARE_NAMESPACE.is_match(line) {
                continue;
            }
            let col = line.find("namespace").unwrap_or(0);
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: idx + 1,
                column: col + 1,
                rule_id: "ts-no-namespace".into(),
                message: "TypeScript `namespace` is a legacy construct — \
                          use ES module `export` / `import` instead."
                    .into(),
                severity: Severity::Warning,
            });
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

    fn run_dts(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("t.d.ts"), source))
    }

    #[test]
    fn flags_namespace() {
        let diags = run("namespace Foo { export const x = 1; }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_export_namespace() {
        let diags = run("export namespace Foo { }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_declare_namespace() {
        assert!(run("declare namespace NodeJS { }").is_empty());
    }

    #[test]
    fn allows_declare_namespace_in_dts() {
        assert!(run_dts("declare namespace NodeJS { }").is_empty());
    }

    #[test]
    fn allows_regular_code() {
        assert!(run("const x = 1;").is_empty());
    }
}
