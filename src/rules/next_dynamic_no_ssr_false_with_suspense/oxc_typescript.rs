//! OXC backend for next-dynamic-no-ssr-false-with-suspense.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::project::Framework;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        if ctx.project.framework != Framework::NextJs {
            return;
        }

        // Callee must be `dynamic`
        let Expression::Identifier(ident) = &call.callee else { return };
        if ident.name.as_str() != "dynamic" {
            return;
        }

        // Need at least 2 arguments
        if call.arguments.len() < 2 {
            return;
        }

        // Second argument must be an object with `ssr: false`
        let Argument::ObjectExpression(obj) = &call.arguments[1] else { return };

        let has_ssr_false = obj.properties.iter().any(|prop| {
            let ObjectPropertyKind::ObjectProperty(p) = prop else { return false };
            let PropertyKey::StaticIdentifier(key) = &p.key else { return false };
            if key.name.as_str() != "ssr" {
                return false;
            }
            let Expression::BooleanLiteral(val) = &p.value else { return false };
            !val.value
        });

        if !has_ssr_false {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "next-dynamic-no-ssr-false-with-suspense".into(),
            message: "Replace `dynamic(..., { ssr: false })` with a `<Suspense>` boundary, or move the lazy import into a client component.".into(),
            severity: Severity::Warning,
            span: Some((call.span.start as usize, (call.span.end - call.span.start) as usize)),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;

    fn next_project() -> ProjectCtx {
        let mut project = ProjectCtx::empty();
        project.framework = Framework::NextJs;
        project
    }

    fn run(source: &str, project: &ProjectCtx) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx_with_project(source, &Check, project)
    }

    #[test]
    fn flags_dynamic_with_ssr_false() {
        let src = "const C = dynamic(() => import('./c'), { ssr: false });";
        assert_eq!(run(src, &next_project()).len(), 1);
    }

    #[test]
    fn allows_dynamic_with_ssr_true() {
        let src = "const C = dynamic(() => import('./c'), { ssr: true });";
        assert!(run(src, &next_project()).is_empty());
    }

    #[test]
    fn allows_dynamic_without_options() {
        let src = "const C = dynamic(() => import('./c'));";
        assert!(run(src, &next_project()).is_empty());
    }

    #[test]
    fn ignores_non_nextjs_project() {
        let src = "const C = dynamic(() => import('./c'), { ssr: false });";
        assert!(run(src, &ProjectCtx::empty()).is_empty());
    }
}
