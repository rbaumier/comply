//! no-duplicate-imports — flag multiple imports from the same module.
//!
//! Walks tree-sitter `import_statement` nodes so multi-line imports and
//! trailing comments cannot fool line-by-line matching.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use rustc_hash::FxHashMap;

#[derive(Debug)]
pub struct Check;

#[derive(Default)]
struct State {
    /// (module, is_type_import) -> first line number
    seen: FxHashMap<(String, bool), usize>,
}

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["import_statement"])
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(State::default()))
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(state) = state.and_then(|s| s.downcast_mut::<State>()) else {
            return;
        };
        let source = ctx.source.as_bytes();
        let Some(source_node) = node.child_by_field_name("source") else {
            return;
        };
        let module = source_node
            .utf8_text(source)
            .unwrap_or("")
            .trim_matches(|c: char| c == '\'' || c == '"')
            .to_string();
        if module.is_empty() {
            return;
        }
        let is_type = node
            .utf8_text(source)
            .unwrap_or("")
            .trim()
            .starts_with("import type ");
        let key = (module.clone(), is_type);
        let pos = node.start_position();
        let line_num = pos.row + 1;
        if let Some(&first_line) = state.seen.get(&key) {
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
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
            state.seen.insert(key, line_num);
        }
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

    #[test]
    fn allows_import_and_import_type_from_same_module() {
        let src = "import { value } from 'bar';\nimport type { Foo } from 'bar';\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_duplicate_type_imports_from_same_module() {
        let src = "import type { Foo } from 'bar';\nimport type { Bar } from 'bar';\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }
}
