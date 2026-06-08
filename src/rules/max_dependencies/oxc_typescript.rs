//! max-dependencies OXC backend — count unique import sources and flag
//! when the count exceeds the configured threshold.

use std::collections::HashSet;
use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};

pub struct Check;

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let max = ctx.config.threshold("max-dependencies", "max", ctx.lang);
        let mut seen: HashSet<&str> = HashSet::new();
        let mut last_import_offset: u32 = 0;

        for node in semantic.nodes().iter() {
            let AstKind::ImportDeclaration(import) = node.kind() else {
                continue;
            };
            let spec = import.source.value.as_str();
            seen.insert(spec);
            last_import_offset = import.span.start;
        }

        if seen.len() > max {
            let (line, column) = byte_offset_to_line_col(ctx.source, last_import_offset as usize);
            return vec![Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: format!(
                    "Maximum number of dependencies ({}) exceeded — this file imports {} modules.",
                    max,
                    seen.len()
                ),
                severity: Severity::Warning,
                span: None,
            }];
        }

        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_too_many_imports() {
        let mut src = String::new();
        for i in 0..16 {
            src.push_str(&format!("import {{ x{i} }} from 'mod-{i}';\n"));
        }
        let diags = run_on(&src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("16 modules"));
    }


    #[test]
    fn allows_within_limit() {
        let mut src = String::new();
        for i in 0..15 {
            src.push_str(&format!("import {{ x{i} }} from 'mod-{i}';\n"));
        }
        assert!(run_on(&src).is_empty());
    }


    #[test]
    fn deduplicates_same_module() {
        let mut src = String::new();
        for i in 0..16 {
            src.push_str(&format!("import {{ x{i} }} from 'same-mod';\n"));
        }
        assert!(run_on(&src).is_empty());
    }
}
