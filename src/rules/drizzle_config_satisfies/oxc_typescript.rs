//! drizzle-config-satisfies — OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn path_is_drizzle_config(path: &std::path::Path) -> bool {
    path.to_string_lossy().contains("drizzle.config")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["drizzle.config"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !path_is_drizzle_config(ctx.path) {
            return;
        }
        let AstKind::VariableDeclarator(decl) = node.kind() else { return };

        // Check if the variable has a `: Config` type annotation.
        let Some(type_ann) = &decl.type_annotation else { return };
        let ty_text = &ctx.source[type_ann.span.start as usize..type_ann.span.end as usize];
        let t = ty_text.trim().trim_start_matches(':').trim();
        if t != "Config" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, decl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `satisfies Config` instead of `: Config` — prefer `export default { ... } satisfies Config`.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::Diagnostic;



    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(src, &Check, "drizzle.config.ts")
    }


    fn run_other(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(src, &Check, "other.ts")
    }


    #[test]
    fn flags_const_config_type_annotation() {
        let src = "const config: Config = { out: './drizzle' }";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_satisfies_config() {
        let src = "export default { out: './drizzle' } satisfies Config";
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_non_drizzle_config_files() {
        let src = "const config: Config = { out: './drizzle' }";
        assert!(run_other(src).is_empty());
    }
}
