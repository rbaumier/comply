//! no-duplicate-imports OXC backend — flag multiple imports from the same module.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::collections::HashMap;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        // (module source, is_type_import) -> first line number
        let mut seen: HashMap<(&str, bool), usize> = HashMap::new();

        for node in semantic.nodes().iter() {
            let AstKind::ImportDeclaration(import) = node.kind() else {
                continue;
            };
            let module = import.source.value.as_str();
            if module.is_empty() {
                continue;
            }
            let is_type = import.import_kind.is_type();
            let key = (module, is_type);
            let (line, column) =
                byte_offset_to_line_col(ctx.source, import.span.start as usize);
            if let Some(&first_line) = seen.get(&key) {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Duplicate import from `{}` \u{2014} already imported on line {}. Merge into a single statement.",
                        module, first_line
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            } else {
                seen.insert(key, line);
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
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
