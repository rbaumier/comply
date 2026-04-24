//! no-duplicate-imports — flag multiple imports from the same module.
//!
//! Walks tree-sitter `import_statement` nodes so multi-line imports and
//! trailing comments cannot fool line-by-line matching.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::collect_nodes_of_kinds;
use std::collections::HashMap;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let mut seen: HashMap<String, usize> = HashMap::new();
        let mut diagnostics = Vec::new();

        for node in collect_nodes_of_kinds(tree, &["import_statement"]) {
            let Some(source_node) = node.child_by_field_name("source") else { continue };
            let module = source_node
                .utf8_text(source)
                .unwrap_or("")
                .trim_matches(|c: char| c == '\'' || c == '"')
                .to_string();
            if module.is_empty() {
                continue;
            }
            let pos = node.start_position();
            let line_num = pos.row + 1;
            if let Some(&first_line) = seen.get(&module) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: line_num,
                    column: pos.column + 1,
                    rule_id: "no-duplicate-imports".into(),
                    message: format!(
                        "Duplicate import from `{}` — already imported on line {}. Merge into a single statement.",
                        module, first_line
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            } else {
                seen.insert(module, line_num);
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_duplicate_imports() {
        let src = "import { a } from 'x';\nimport { b } from 'x';\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Duplicate import from `x`"));
        assert_eq!(diags[0].line, 2);
    }

    #[test]
    fn allows_distinct_sources() {
        let src = "import { a } from 'x';\nimport { b } from 'y';\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_three_duplicates() {
        let src = "import { a } from 'lodash';\n\
                   import { b } from 'lodash';\n\
                   import { c } from 'lodash';\n";
        let diags = run(src);
        assert_eq!(diags.len(), 2);
    }

    // Regression: line-by-line matching missed duplicates where the first
    // import spanned multiple lines — the `from 'x'` lived on a later line
    // than `import`, breaking the `line.starts_with("import ")` check.
    #[test]
    fn flags_duplicate_after_multiline_import() {
        let src = "import {\n\
                     a,\n\
                     b,\n\
                   } from 'x';\n\
                   import { c } from 'x';\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Duplicate import from `x`"));
    }

    #[test]
    fn flags_two_multiline_imports_same_source() {
        let src = "import {\n  a,\n} from 'x';\nimport {\n  b,\n} from 'x';\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }
}
