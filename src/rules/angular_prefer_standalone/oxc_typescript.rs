use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

fn is_angular_file(source: &str) -> bool {
    source.contains("@angular/") || source.contains("@Component")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Decorator]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@Component"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::Decorator(decorator) = node.kind() else {
            return;
        };
        if !is_angular_file(ctx.source) {
            return;
        }
        let Expression::CallExpression(call) = &decorator.expression else {
            return;
        };
        let Expression::Identifier(id) = &call.callee else {
            return;
        };
        if id.name.as_str() != "Component" {
            return;
        }
        let start = decorator.span.start as usize;
        let end = decorator.span.end as usize;
        let text = &ctx.source[start..end];
        if text.contains("standalone") {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, start);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: "angular-prefer-standalone".into(),
            message: "`@Component` without `standalone: true` — prefer standalone components over NgModule declarations (Angular 15+).".into(),
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
    fn flags_component_without_standalone() {
        let src = "import { Component } from '@angular/core';\n@Component({ template: 'x' }) class C {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_standalone_component() {
        let src = "import { Component } from '@angular/core';\n@Component({ standalone: true, template: 'x' }) class C {}";
        assert!(run(src).is_empty());
    }
}
