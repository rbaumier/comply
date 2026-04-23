//! import-dedupe backend — flag duplicate specifiers within one import.
//!
//! `import { a, a } from 'x'` — the second `a` is redundant.
//! Detection compares the **local binding name** (alias if present, else
//! the imported name) so `import { a, a as b }` is allowed while
//! `import { a as x, a as x }` is flagged.

use crate::diagnostic::{Diagnostic, Severity};
use std::collections::HashSet;

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "import_statement" {
        return;
    }
    // Find the named_imports block inside the import_clause.
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() != "import_clause" {
            continue;
        }
        let mut cc = child.walk();
        for clause_child in child.named_children(&mut cc) {
            if clause_child.kind() != "named_imports" {
                continue;
            }
            let mut seen: HashSet<String> = HashSet::new();
            let mut spec_cursor = clause_child.walk();
            for spec in clause_child.named_children(&mut spec_cursor) {
                if spec.kind() != "import_specifier" {
                    continue;
                }
                // Local binding = alias if present, else name.
                let local = spec
                    .child_by_field_name("alias")
                    .or_else(|| spec.child_by_field_name("name"));
                let Some(local) = local else { continue };
                let Ok(local_name) = local.utf8_text(source) else { continue };
                if !seen.insert(local_name.to_string()) {
                    let pos = spec.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "import-dedupe".into(),
                        message: format!(
                            "Duplicate specifier `{local_name}` in the same import — remove the redundant entry."
                        ),
                        severity: Severity::Warning,
                        span: Some((spec.start_byte(), spec.end_byte() - spec.start_byte())),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_duplicate_named_specifier() {
        let d = run_on("import { a, a } from 'x';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Duplicate specifier `a`"));
    }

    #[test]
    fn flags_duplicate_alias() {
        let d = run_on("import { a as x, b as x } from 'x';");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`x`"));
    }

    #[test]
    fn allows_distinct_specifiers() {
        assert!(run_on("import { a, b } from 'x';").is_empty());
    }

    #[test]
    fn allows_alias_with_same_source_name() {
        // `a` and `a as b` bind different locals: `a` and `b`.
        assert!(run_on("import { a, a as b } from 'x';").is_empty());
    }
}
