//! no-index-file backend — flag `index.{ts,tsx,js,jsx,mjs}` files that act
//! as barrels (contain re-exports from sibling modules).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

const INDEX_FILENAMES: &[&str] = &[
    "index.ts",
    "index.tsx",
    "index.js",
    "index.jsx",
    "index.mjs",
    "index.cjs",
];

/// True if the line looks like a re-export:
/// - `export * from '...'`
/// - `export * as X from '...'`
/// - `export { ... } from '...'`
fn is_re_export(line: &str) -> bool {
    let trimmed = line.trim_start();
    if !trimmed.starts_with("export") {
        return false;
    }
    // Cheap: a re-export always contains ` from ` followed by a quote.
    // `export * from '...'`, `export { x } from "..."`.
    let Some(from_idx) = trimmed.find(" from ") else {
        return false;
    };
    let after = &trimmed[from_idx + 6..];
    after.trim_start().starts_with('\'') || after.trim_start().starts_with('"')
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let Some(name) = ctx.path.file_name().and_then(|f| f.to_str()) else {
            return Vec::new();
        };
        if !INDEX_FILENAMES.contains(&name) {
            return Vec::new();
        }

        // Fire once if any re-export line is found.
        for (i, line) in ctx.source.lines().enumerate() {
            if is_re_export(line) {
                return vec![Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i + 1,
                    column: 1,
                    rule_id: "no-index-file".into(),
                    message: format!(
                        "`{name}` is a barrel file — re-exports cause bundler bloat and \
                         circular-import risk. Import from the defining module instead."
                    ),
                    severity: Severity::Warning,
                    span: None,
                }];
            }
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(path: &str, source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
    }

    #[test]
    fn flags_index_ts_with_reexport_all() {
        let diags = run("index.ts", "export * from './foo';\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule_id, "no-index-file");
    }

    #[test]
    fn flags_index_ts_with_named_reexport() {
        let diags = run("index.ts", "export { foo } from './foo';\n");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_index_js_barrel() {
        let diags = run("index.js", "export * from './bar';\n");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_index_ts_with_implementation() {
        let src = "export function hello() { return 42; }\n";
        assert!(run("index.ts", src).is_empty());
    }

    #[test]
    fn ignores_non_index_file() {
        let diags = run("foo.ts", "export * from './bar';\n");
        assert!(diags.is_empty());
    }

    #[test]
    fn allows_index_with_export_const() {
        // No `from '...'` → not a re-export.
        let src = "export const x = 1;\n";
        assert!(run("index.ts", src).is_empty());
    }
}
