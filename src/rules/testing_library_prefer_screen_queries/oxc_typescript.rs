//! testing-library-prefer-screen-queries oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{BindingPattern, Expression, PropertyKey};
use std::sync::Arc;

pub struct Check;

fn is_query_name(name: &str) -> bool {
    name.starts_with("getBy")
        || name.starts_with("getAllBy")
        || name.starts_with("queryBy")
        || name.starts_with("queryAllBy")
        || name.starts_with("findBy")
        || name.starts_with("findAllBy")
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::VariableDeclarator]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["render("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::VariableDeclarator(decl) = node.kind() else {
            return;
        };
        // LHS must be an object pattern.
        let BindingPattern::ObjectPattern(obj_pat) = &decl.id else {
            return;
        };
        // RHS must be `render(...)`.
        let Some(init) = &decl.init else {
            return;
        };
        let Expression::CallExpression(call) = init else {
            return;
        };
        let Expression::Identifier(callee) = &call.callee else {
            return;
        };
        if callee.name.as_str() != "render" {
            return;
        }
        // Any destructured property whose key is a testing-library query?
        let mut found: Option<String> = None;
        for prop in &obj_pat.properties {
            let PropertyKey::StaticIdentifier(key) = &prop.key else {
                continue;
            };
            if is_query_name(key.name.as_str()) {
                found = Some(key.name.to_string());
                break;
            }
        }
        let Some(name) = found else { return };
        let (line, column) = byte_offset_to_line_col(ctx.source, decl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Destructuring `{name}` from `render(...)` — use `screen.{name}` \
                 instead. Stable across refactors, less repetition."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}
