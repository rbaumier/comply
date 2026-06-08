//! angular-prefer-inject OXC backend — prefer `inject()` over constructor DI.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{ClassElement, Expression, FormalParameterKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

const ANGULAR_DECORATORS: &[&str] = &["Component", "Injectable", "Directive", "Pipe"];

fn is_angular_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@angular/")
        || crate::oxc_helpers::source_contains(source, "@Component")
        || crate::oxc_helpers::source_contains(source, "@Injectable")
        || crate::oxc_helpers::source_contains(source, "@Directive")
}

/// Check if a class has an Angular decorator.
fn class_has_angular_decorator(class: &oxc_ast::ast::Class) -> bool {
    for dec in &class.decorators {
        let name = match &dec.expression {
            Expression::Identifier(id) => Some(id.name.as_str()),
            Expression::CallExpression(call) => {
                if let Expression::Identifier(id) = &call.callee {
                    Some(id.name.as_str())
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(n) = name {
            if ANGULAR_DECORATORS.contains(&n) {
                return true;
            }
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Class]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@Component", "@Directive", "@Injectable", "@Pipe"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_angular_file(ctx.source) {
            return;
        }
        let AstKind::Class(class) = node.kind() else {
            return;
        };
        if !class_has_angular_decorator(class) {
            return;
        }

        // Find the constructor and check for parameter properties.
        for element in &class.body.body {
            let ClassElement::MethodDefinition(method) = element else {
                continue;
            };
            let PropertyKey::StaticIdentifier(key) = &method.key else {
                continue;
            };
            if key.name.as_str() != "constructor" {
                continue;
            };
            let Some(func) = &method.value.body else {
                continue;
            };
            // Check formal parameters for accessibility modifiers (parameter properties).
            if method.value.params.kind == FormalParameterKind::FormalParameter {
                for param in &method.value.params.items {
                    if param.accessibility.is_some() || param.readonly {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, param.span.start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message:
                                "Constructor parameter property — prefer the `inject()` function (Angular 14+)."
                                    .into(),
                            severity: Severity::Warning,
                            span: None,
                        });
                    }
                }
            }
            // We only care about the constructor body existing, already checked params
            let _ = func;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_constructor_param_property_in_component() {
        let src = "import { Component } from '@angular/core';\n@Component({}) class C { constructor(private svc: Svc) {} }";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_inject_function() {
        let src = "import { Component, inject } from '@angular/core';\n@Component({}) class C { svc = inject(Svc); }";
        assert!(run(src).is_empty());
    }


    #[test]
    fn ignores_non_angular_classes() {
        let src = "class C { constructor(private svc: Svc) {} }";
        assert!(run(src).is_empty());
    }
}
