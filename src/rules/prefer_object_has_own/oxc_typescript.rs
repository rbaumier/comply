//! OXC backend for prefer-object-has-own.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["hasOwnProperty"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else { return };

        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name != "hasOwnProperty" {
            return;
        }

        // Allow Object.prototype.hasOwnProperty.call pattern.
        let obj_span = member.object.span();
        let obj_text = &ctx.source[obj_span.start as usize..obj_span.end as usize];
        if obj_text == "Object.prototype" {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Use `Object.hasOwn(obj, key)` instead of `obj.hasOwnProperty(key)` (ES2022)."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    fn run(code: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(code, &Check)
    }


    #[test]
    fn flags_has_own_property() {
        assert_eq!(run("obj.hasOwnProperty('key')").len(), 1);
    }


    #[test]
    fn flags_this_has_own_property() {
        assert_eq!(run("this.hasOwnProperty('key')").len(), 1);
    }


    #[test]
    fn allows_object_has_own() {
        assert!(run("Object.hasOwn(obj, 'key')").is_empty());
    }


    #[test]
    fn allows_prototype_call() {
        assert!(run("Object.prototype.hasOwnProperty.call(obj, 'key')").is_empty());
    }
}
