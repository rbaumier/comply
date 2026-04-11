//! require-module-attributes backend — flag import/export statements that
//! have an empty `with {}` attribute clause.
//!
//! Matches patterns like:
//! - `import data from './data.json' with {}`
//! - `import data from './data.json' with { }`
//! - `export { foo } from './bar' with {}`

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};

#[derive(Debug)]
pub struct Check;

/// Check if a line has an import/export with an empty `with {}` clause.
fn has_empty_with_clause(line: &str) -> bool {
    let trimmed = line.trim();

    // Must be an import or export statement with a source string
    let is_import_export = trimmed.starts_with("import ") || trimmed.starts_with("export ");
    if !is_import_export {
        return false;
    }

    // Find `with` keyword followed by empty braces
    // We look for the pattern: with { } or with{}
    if let Some(with_pos) = trimmed.find(" with ") {
        let after_with = trimmed[with_pos + 6..].trim();
        if let Some(rest) = after_with.strip_prefix('{') {
            let after_brace = rest.trim_start();
            if after_brace.starts_with('}') {
                return true;
            }
        }
    }

    // Also match `with{}` (no space after `with`)
    if let Some(with_pos) = trimmed.find(" with{") {
        let after_with = trimmed[with_pos + 5..].trim_start();
        if let Some(rest) = after_with.strip_prefix('{') {
            let after_brace = rest.trim_start();
            if after_brace.starts_with('}') {
                return true;
            }
        }
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        ctx.source
            .lines()
            .enumerate()
            .filter(|(_, line)| has_empty_with_clause(line))
            .map(|(idx, line)| {
                let stmt_type = if line.trim().starts_with("import") {
                    "import"
                } else {
                    "export"
                };
                Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "require-module-attributes".into(),
                    message: format!(
                        "{stmt_type} statement has an empty `with {{}}` clause — \
                         add the required attributes or remove the clause."
                    ),
                    severity: Severity::Warning,
                }
            })
            .collect()
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
    fn flags_import_with_empty_attributes() {
        let diags = run("import data from './data.json' with {};");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("import"));
    }

    #[test]
    fn flags_import_with_empty_spaced_attributes() {
        let diags = run("import data from './data.json' with {  };");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_export_with_empty_attributes() {
        let diags = run("export { foo } from './bar' with {};");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("export"));
    }

    #[test]
    fn allows_import_with_attributes() {
        assert!(run("import data from './data.json' with { type: 'json' };").is_empty());
    }

    #[test]
    fn allows_import_without_with_clause() {
        assert!(run("import { foo } from './foo';").is_empty());
    }

    #[test]
    fn allows_regular_code() {
        assert!(run("const x = 1;").is_empty());
    }

    #[test]
    fn flags_export_all_with_empty_attributes() {
        let diags = run("export * from './mod' with {};");
        assert_eq!(diags.len(), 1);
    }
}
