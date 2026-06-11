//! jsx-no-new-function-as-prop OxcCheck backend. Files importing from
//! `solid-js` are exempt: SolidJS components do not re-render, so inline JSX
//! functions never cause extra renders and `useCallback` does not apply.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::JSXExpression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if ctx.source_contains("solid-js") {
            return;
        }
        let AstKind::JSXAttribute(attr) = node.kind() else { return };
        let oxc_ast::ast::JSXAttributeName::Identifier(name_ident) = &attr.name else { return };
        let attr_name = name_ident.name.as_str();

        let Some(oxc_ast::ast::JSXAttributeValue::ExpressionContainer(container)) = &attr.value
        else {
            return;
        };

        let kind_label = match &container.expression {
            JSXExpression::ArrowFunctionExpression(_) => "arrow function",
            JSXExpression::FunctionExpression(_) => "function expression",
            _ => return,
        };

        let (line, column) =
            byte_offset_to_line_col(ctx.source, container.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "{kind_label} as value of JSX prop `{attr_name}` creates a new reference every render — hoist with `useCallback` or to a stable handler."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_arrow_in_jsx_prop_react() {
        let src = "const a = <button onClick={() => f()} />;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_arrow_in_jsx_prop_solid() {
        let src = "import { createSignal } from \"solid-js\";\nconst a = <button onClick={() => f()} />;";
        assert!(run(src).is_empty());
    }
}
