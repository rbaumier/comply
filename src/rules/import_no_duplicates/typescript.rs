//! import-no-duplicates backend — flag multiple imports from the same module.
//!
//! Why: duplicate imports from a single module fragment the import list,
//! hide the full surface being pulled in, and encourage drift between the
//! two sites (one updated, the other forgotten). Merging them into a
//! single statement keeps the dependency footprint of the file visible
//! at a glance.

use std::collections::HashMap;

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let mut seen: HashMap<(String, bool), usize> = HashMap::new();
    let mut cursor = node.walk();

    for child in node.children(&mut cursor) {
        if child.kind() != "import_statement" {
            continue;
        }
        let Some(source_node) = child.child_by_field_name("source") else {
            continue;
        };
        let Ok(spec) = std::str::from_utf8(&source[source_node.byte_range()]) else {
            continue;
        };
        let spec_clean = spec.trim_matches(|c| c == '"' || c == '\'' || c == '`');
        let is_type = child
            .utf8_text(source)
            .unwrap_or("")
            .trim()
            .starts_with("import type ");
        let key = (spec_clean.to_string(), is_type);

        if let Some(&first_line) = seen.get(&key) {
            let pos = child.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "import-no-duplicates".into(),
                message: format!(
                    "Module `{spec_clean}` is imported multiple times (first at line {first_line}). Merge into a single import.",
                ),
                severity: Severity::Warning,
                span: None,
            });
        } else {
            seen.insert(key, child.start_position().row + 1);
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
    fn flags_duplicate_imports() {
        let src = "import { foo } from './utils';\nimport { bar } from './utils';";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("./utils"));
    }

    #[test]
    fn allows_different_sources() {
        let src = "import { foo } from './utils';\nimport { bar } from './other';";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_import_and_import_type_from_same_module() {
        let src = "import { foo } from './utils';\nimport type { FooType } from './utils';";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_duplicate_type_imports() {
        let src = "import type { Foo } from './utils';\nimport type { Bar } from './utils';";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_single_import() {
        assert!(run_on("import { foo, bar } from './utils';").is_empty());
    }

    #[test]
    fn flags_side_effect_duplicate() {
        let src = "import './init';\nimport './init';";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_triple_import() {
        let src = "import { a } from './m';\nimport { b } from './m';\nimport { c } from './m';";
        let d = run_on(src);
        assert_eq!(d.len(), 2);
    }
}
