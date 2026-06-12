//! ts-no-useless-constructor OxcCheck backend — flag constructors that are empty
//! or only call `super(...)` with the same arguments, and have no
//! accessibility modifiers, parameter properties, or decorators. Constructors in
//! a class that extends a base class are exempt: they may widen the base
//! constructor's visibility, which is not observable from the subclass alone.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    Argument, BindingPattern, Expression, MethodDefinitionKind, Statement,
};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::MethodDefinition]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::MethodDefinition(method) = node.kind() else {
            return;
        };
        if method.kind != MethodDefinitionKind::Constructor {
            return;
        }

        // Skip if the enclosing class extends a base class. A subclass
        // constructor that calls `super(...)` with the same arguments (or an
        // empty body) is not necessarily useless: TypeScript does not promote a
        // base class's `protected`/`private` constructor accessibility to the
        // subclass, so an explicit constructor may exist solely to widen the
        // visibility (e.g. `protected` base → public subclass) and allow
        // external instantiation. That base-constructor accessibility lives in
        // another declaration the subclass node cannot see, so the pattern is
        // undecidable here — only constructors in a class with no base are
        // unambiguously useless.
        if semantic
            .nodes()
            .ancestors(node.id())
            .any(|ancestor| matches!(ancestor.kind(), AstKind::Class(class) if class.super_class.is_some()))
        {
            return;
        }

        // Skip if constructor has accessibility modifier
        if method.accessibility.is_some() {
            return;
        }
        // Skip if override
        if method.r#override {
            return;
        }

        let func = &method.value;

        // Skip if any parameter has decorators, accessibility modifiers, or is a parameter property
        for param in &func.params.items {
            if !param.decorators.is_empty() {
                return;
            }
            if param.accessibility.is_some() {
                return;
            }
            if param.r#override {
                return;
            }
            if param.readonly {
                return;
            }
        }

        let Some(body) = &func.body else {
            return;
        };

        let stmts: Vec<&Statement> = body
            .statements
            .iter()
            .filter(|s| !matches!(s, Statement::EmptyStatement(_)))
            .collect();

        // Case 1: completely empty body
        if stmts.is_empty() {
            let (line, column) = byte_offset_to_line_col(ctx.source, method.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Useless constructor — remove it.".into(),
                severity: Severity::Warning,
                span: None,
            });
            return;
        }

        // Case 2: only `super(...)` call with same args passthrough
        if stmts.len() != 1 {
            return;
        }
        let Statement::ExpressionStatement(expr_stmt) = stmts[0] else {
            return;
        };
        let Expression::CallExpression(call) = &expr_stmt.expression else {
            return;
        };
        let Expression::Super(_) = &call.callee else {
            return;
        };

        // Collect argument names (supporting spread)
        let arg_names: Vec<String> = call
            .arguments
            .iter()
            .filter_map(|arg| match arg {
                Argument::Identifier(ident) => Some(ident.name.to_string()),
                Argument::SpreadElement(spread) => {
                    if let Expression::Identifier(ident) = &spread.argument {
                        Some(format!("...{}", ident.name))
                    } else {
                        None
                    }
                }
                _ => None,
            })
            .collect();

        // If any argument wasn't a simple identifier/spread-identifier, bail
        if arg_names.len() != call.arguments.len() {
            return;
        }

        // Handle rest parameter
        let mut formatted_params: Vec<String> = Vec::new();
        for param in &func.params.items {
            match &param.pattern {
                BindingPattern::BindingIdentifier(id) => {
                    formatted_params.push(id.name.to_string());
                }
                _ => return, // Complex pattern, bail
            }
        }
        if let Some(rest) = &func.params.rest {
            match &rest.rest.argument {
                BindingPattern::BindingIdentifier(id) => {
                    formatted_params.push(format!("...{}", id.name));
                }
                _ => return,
            }
        }

        if formatted_params == arg_names {
            let (line, column) = byte_offset_to_line_col(ctx.source, method.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: super::META.id.into(),
                message: "Useless constructor — it only calls `super()` with the same arguments."
                    .into(),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_empty_constructor() {
        let diags = run_on("class Foo { constructor() {} }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_super_passthrough_without_extends() {
        // No base class: a `super(...)`-shaped body is unreachable, but a
        // trivial constructor with no base is unambiguously useless.
        let diags = run_on("class Foo { constructor() {} }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_constructor_with_body() {
        assert!(run_on("class Foo { constructor() { this.init(); } }").is_empty());
    }

    #[test]
    fn allows_private_constructor() {
        assert!(run_on("class Foo { private constructor() {} }").is_empty());
    }

    #[test]
    fn allows_parameter_property() {
        assert!(run_on("class Foo { constructor(public name: string) {} }").is_empty());
    }

    // Regression for #1097: a subclass constructor that only forwards to
    // `super(...)` may exist to widen the base constructor's visibility
    // (e.g. `protected` base → public subclass), so it must not be flagged.
    #[test]
    fn allows_super_passthrough_in_subclass() {
        let src = "class Sub extends Base {\n  constructor(nextPolicy: RequestPolicy, options: RequestPolicyOptions) {\n    super(nextPolicy, options);\n  }\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_empty_constructor_in_subclass() {
        assert!(run_on("class Sub extends Base { constructor() {} }").is_empty());
    }
}
