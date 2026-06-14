//! react-forward-ref-uses-ref oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, FunctionBody, ImportDeclarationSpecifier, Program, Statement};
use std::sync::Arc;

pub struct Check;

/// React's `forwardRef` is exported from `react`; `react-dom` re-exports the
/// React namespace, so both are in-scope. Other packages — notably
/// `@nestjs/common` and `@angular/core`, whose `forwardRef(() => Token)`
/// resolves circular DI — are unrelated APIs and must not be flagged.
fn is_react_source(source: &str) -> bool {
    source == "react" || source == "react-dom"
}

/// The local binding `callee` of a `forwardRef(...)` / `<ns>.forwardRef(...)`
/// call. For a bare call this is the named binding `forwardRef`; for a member
/// call it is the namespace/default object (e.g. `React` in `React.forwardRef`).
/// Returns `None` for shapes this rule does not recognise.
fn forward_ref_binding<'a>(callee: &Expression<'a>) -> Option<&'a str> {
    match callee {
        Expression::Identifier(id) if id.name.as_str() == "forwardRef" => Some("forwardRef"),
        Expression::StaticMemberExpression(member)
            if member.property.name.as_str() == "forwardRef" =>
        {
            match &member.object {
                Expression::Identifier(obj) => Some(obj.name.as_str()),
                _ => None,
            }
        }
        _ => None,
    }
}

/// True when `binding` is introduced by an `import` from React. Keying on the
/// binding's import provenance — not the literal name — lets a file import both
/// React's and NestJS's `forwardRef` and only flag the React one.
fn binding_imported_from_react(program: &Program<'_>, binding: &str) -> bool {
    program.body.iter().any(|stmt| {
        let Statement::ImportDeclaration(import) = stmt else {
            return false;
        };
        if !is_react_source(import.source.value.as_str()) {
            return false;
        }
        let Some(specifiers) = &import.specifiers else {
            return false;
        };
        specifiers.iter().any(|spec| {
            let local = match spec {
                ImportDeclarationSpecifier::ImportSpecifier(s) => s.local.name.as_str(),
                ImportDeclarationSpecifier::ImportDefaultSpecifier(s) => s.local.name.as_str(),
                ImportDeclarationSpecifier::ImportNamespaceSpecifier(s) => s.local.name.as_str(),
            };
            local == binding
        })
    })
}

/// A deprecated API-compatibility stub keeps the `forwardRef` signature for
/// backward compatibility but is a no-op: it emits a deprecation warning and
/// returns `null`, so it cannot meaningfully forward a ref. Such bodies consist
/// solely of `warn()`/`console.warn(...)`-style calls and a `return null`.
fn is_deprecation_stub(body: &FunctionBody) -> bool {
    let mut returns_null = false;
    let mut warns = false;
    for stmt in &body.statements {
        match stmt {
            Statement::ExpressionStatement(expr_stmt) => {
                if is_warn_call(&expr_stmt.expression) {
                    warns = true;
                } else {
                    return false;
                }
            }
            Statement::ReturnStatement(ret) => match &ret.argument {
                Some(Expression::NullLiteral(_)) => returns_null = true,
                _ => return false,
            },
            _ => return false,
        }
    }
    warns && returns_null
}

/// Recognizes deprecation-warning calls: a bare `warn(...)` identifier callee
/// or a `console.warn(...)`/`console.error(...)` member callee.
fn is_warn_call(expr: &Expression) -> bool {
    let Expression::CallExpression(call) = expr else {
        return false;
    };
    match &call.callee {
        Expression::Identifier(id) => id.name == "warn",
        Expression::StaticMemberExpression(m) => {
            matches!(m.property.name.as_str(), "warn" | "error")
                && matches!(&m.object, Expression::Identifier(obj) if obj.name == "console")
        }
        _ => false,
    }
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["forwardRef"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Only React's `forwardRef` wraps a component that should accept a `ref`.
        // Same-named DI helpers from `@nestjs/common` / `@angular/core` take a
        // zero-arg factory, so gate on the binding's import source.
        let Some(binding) = forward_ref_binding(&call.callee) else {
            return;
        };
        if !binding_imported_from_react(semantic.nodes().program(), binding) {
            return;
        }

        // Get the first argument (the render function).
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let Some(expr) = first_arg.as_expression() else {
            return;
        };

        let (param_count, stub) = match expr {
            Expression::ArrowFunctionExpression(arrow) => {
                (arrow.params.items.len(), is_deprecation_stub(&arrow.body))
            }
            Expression::FunctionExpression(func) => (
                func.params.items.len(),
                func.body.as_ref().is_some_and(|b| is_deprecation_stub(b)),
            ),
            _ => return,
        };

        if stub {
            return;
        }

        if param_count < 2 {
            let (line, column) =
                byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "`forwardRef` component is missing the `ref` parameter.".into(),
                severity: Severity::Warning,
                span: None,
            });
        }
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_missing_ref_param() {
        let src = "import * as React from \"react\";\nconst Comp = React.forwardRef((props) => <div />);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_ref_param() {
        let src = "import * as React from \"react\";\nconst Comp = React.forwardRef((props, ref) => <div ref={ref} />);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_no_params() {
        let src = "import * as React from \"react\";\nconst Comp = React.forwardRef(() => <div />);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_deprecation_stub_warn_return_null() {
        // Regression for issue #2013: deprecated API-compatibility stubs keep the
        // `forwardRef` signature but are no-ops that warn and return null.
        let src = "import * as React from \"react\";\nconst CalendarPickerSkeleton = React.forwardRef(function DeprecatedCalendarPickerSkeleton() {\n  warn();\n  return null;\n}) as CalendarPickerSkeletonComponent;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_deprecation_stub_console_warn() {
        let src = "import * as React from \"react\";\nconst Old = React.forwardRef(() => {\n  console.warn('deprecated');\n  return null;\n});";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_component_that_forgets_ref_without_deprecation() {
        // A real mistake: returns JSX, never uses ref, no deprecation marker — still fires.
        let src = "import * as React from \"react\";\nconst Comp = React.forwardRef(function Comp(props) {\n  return <div />;\n});";
        assert_eq!(run(src).len(), 1);
    }

    // Regression for #2398: NestJS's `forwardRef(() => Service)` from
    // `@nestjs/common` resolves circular DI — a different API that shares the
    // name. It must not be flagged for a missing `ref` parameter.
    #[test]
    fn ignores_nestjs_forward_ref() {
        let src = r#"
            import { Module, forwardRef } from "@nestjs/common";
            @Module({ imports: [forwardRef(() => CircularModule)] })
            export class InputModule {}
        "#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }

    // Negative space: a genuine React `forwardRef` imported from `react` and used
    // without a `ref` param must still be flagged.
    #[test]
    fn flags_react_forward_ref_missing_ref_param() {
        let src = r#"
            import { forwardRef } from "react";
            const Btn = forwardRef((props) => <div />);
        "#;
        assert_eq!(run(src).len(), 1, "{:?}", run(src));
    }

    // A bare `forwardRef(...)` with no import cannot be proven to be React's, so
    // it must not be flagged.
    #[test]
    fn ignores_forward_ref_without_import() {
        let src = r#"const Btn = forwardRef((props) => <div />);"#;
        assert!(run(src).is_empty(), "{:?}", run(src));
    }
}
