//! ts-no-empty-object-type backend — detect `{}` in type annotation positions.
//!
//! We look for `: {}`, `< {}>`, `as {}`, `| {}`, `& {}` patterns to catch
//! empty object types in annotation context without false-positiving on
//! empty object literals.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use regex::Regex;
use std::sync::LazyLock;

#[derive(Debug)]
pub struct Check;

static EMPTY_OBJ_TYPE: LazyLock<Regex> = LazyLock::new(|| {
    // Match `{}` preceded by type-annotation indicators
    Regex::new(r"(?::\s*|[<,]\s*|as\s+|[|&]\s*)\{\s*\}").unwrap()
});

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            // Skip lines that are clearly inside intersection types (common in utility types)
            for m in EMPTY_OBJ_TYPE.find_iter(line) {
                // Find column of the `{`
                let brace_offset = line[m.start()..].find('{').unwrap_or(0) + m.start();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: brace_offset + 1,
                    rule_id: "ts-no-empty-object-type".into(),
                    message: "`{}` as a type matches any non-nullish value. \
                              Use `Record<string, never>` for an empty object, \
                              or `object` / `unknown`."
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
    fn flags_empty_object_type_annotation() {
        let diags = run("const x: {} = {};");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_object_in_generic() {
        let diags = run("type X = Map<string, {}>;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_empty_object_in_union() {
        let diags = run("type X = string | {};");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_non_empty_object_type() {
        assert!(run("const x: { a: number } = { a: 1 };").is_empty());
    }

    #[test]
    fn allows_empty_object_literal() {
        assert!(run("const x = {};").is_empty());
    }
}
