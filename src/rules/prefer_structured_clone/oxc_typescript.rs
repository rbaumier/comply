//! prefer-structured-clone OXC backend — flag `JSON.parse(JSON.stringify(…))`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["JSON.parse"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Callee must be JSON.parse
        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "parse" {
            return;
        }
        let oxc_ast::ast::Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "JSON" {
            return;
        }

        // Must have exactly one argument
        if call.arguments.len() != 1 {
            return;
        }

        // The argument must be a call to JSON.stringify with exactly one arg
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let inner_call = match first_arg {
            oxc_ast::ast::Argument::CallExpression(c) => c,
            _ => return,
        };
        let oxc_ast::ast::Expression::StaticMemberExpression(inner_member) =
            &inner_call.callee
        else {
            return;
        };
        if inner_member.property.name.as_str() != "stringify" {
            return;
        }
        let oxc_ast::ast::Expression::Identifier(inner_obj) = &inner_member.object else {
            return;
        };
        if inner_obj.name.as_str() != "JSON" {
            return;
        }
        if inner_call.arguments.len() != 1 {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `structuredClone(…)` over `JSON.parse(JSON.stringify(…))` to create a deep clone.".into(),
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_json_parse_stringify() {
        let d = run_on("const copy = JSON.parse(JSON.stringify(obj));");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("structuredClone"));
    }

    #[test]
    fn flags_nested_expression() {
        let d = run_on("return JSON.parse(JSON.stringify(this.state));");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_structured_clone() {
        assert!(run_on("const copy = structuredClone(obj);").is_empty());
    }

    #[test]
    fn allows_json_parse_alone() {
        assert!(run_on("const data = JSON.parse(text);").is_empty());
    }

    #[test]
    fn allows_json_stringify_alone() {
        assert!(run_on("const text = JSON.stringify(obj);").is_empty());
    }

    #[test]
    fn allows_stringify_replacer() {
        assert!(
            run_on("const copy = JSON.parse(JSON.stringify(obj, replacer));").is_empty()
        );
    }

    #[test]
    fn allows_parse_reviver() {
        assert!(
            run_on("const copy = JSON.parse(JSON.stringify(obj), reviver);").is_empty()
        );
    }
}
