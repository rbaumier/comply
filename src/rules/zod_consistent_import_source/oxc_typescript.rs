use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_zod_subpath(spec: &str) -> bool {
    spec.starts_with("zod/")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else {
            return;
        };
        let src_value = import.source.value.as_str();
        if !is_zod_subpath(src_value) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, import.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Import from '{src_value}' uses a non-standard zod subpath. Use consistent import source for zod."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_zod_v4_import() {
        let d = run_on("import { z } from 'zod/v4';");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_zod_mini_import() {
        let d = run_on("import { z } from 'zod/mini';");
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_main_zod_import() {
        assert!(run_on("import { z } from 'zod';").is_empty());
    }


    #[test]
    fn allows_scoped_zod_package() {
        assert!(run_on("import { foo } from '@zod/utils';").is_empty());
    }
}
