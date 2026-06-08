use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ExportAllDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ExportAllDeclaration(decl) = node.kind() else { return };
        // Allow namespace re-exports: `export * as ns from '...'`
        if decl.exported.is_some() {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, decl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Avoid `export * from '...'` \u{2014} use named re-exports instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_star_reexport() {
        assert_eq!(run("export * from './foo';").len(), 1);
    }


    #[test]
    fn flags_star_reexport_double_quotes() {
        assert_eq!(run("export * from \"./foo\";").len(), 1);
    }


    #[test]
    fn allows_named_reexport() {
        assert!(run("export { foo, bar } from './foo';").is_empty());
    }


    #[test]
    fn allows_namespace_reexport() {
        assert!(run("export * as foo from './foo';").is_empty());
    }


    #[test]
    fn allows_local_named_export() {
        assert!(run("export function foo() {}").is_empty());
    }


    #[test]
    fn allows_default_export() {
        assert!(run("export default function foo() {}").is_empty());
    }
}
