//! elysia-aot-dynamic-route OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const ROUTE_METHODS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "all", "head", "options", "route",
];

fn is_dynamic_path(text: &str, kind: &str) -> bool {
    match kind {
        "template" => text.contains("${"),
        "binary" => text.contains('+'),
        _ => false,
    }
}

fn imports_elysia(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "from 'elysia'")
        || crate::oxc_helpers::source_contains(source, "from \"elysia\"")
        || crate::oxc_helpers::source_contains(source, "from 'elysia/")
        || crate::oxc_helpers::source_contains(source, "from \"elysia/")
        || crate::oxc_helpers::source_contains(source, "from '@elysiajs/")
        || crate::oxc_helpers::source_contains(source, "from \"@elysiajs/")
}

fn is_test_file(path: &std::path::Path) -> bool {
    let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if name.contains(".test.") || name.contains(".spec.") {
        return true;
    }
    path.components().any(|c| {
        matches!(
            c.as_os_str().to_str(),
            Some("__tests__") | Some("__test__") | Some("tests") | Some("test")
        )
    })
}

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
        if !ctx.project.has_framework("elysia") {
            return;
        }
        if !imports_elysia(ctx.source) {
            return;
        }
        if is_test_file(ctx.path) {
            return;
        }

        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let method_name = member.property.name.as_str();
        if !ROUTE_METHODS.contains(&method_name) {
            return;
        }

        let Some(first_arg) = call.arguments.first() else {
            return;
        };

        let (is_dynamic, arg_span) = match first_arg {
            oxc_ast::ast::Argument::TemplateLiteral(tpl) => {
                let text = &ctx.source[tpl.span.start as usize..tpl.span.end as usize];
                (is_dynamic_path(text, "template"), tpl.span)
            }
            oxc_ast::ast::Argument::BinaryExpression(bin) => {
                let text = &ctx.source[bin.span.start as usize..bin.span.end as usize];
                (is_dynamic_path(text, "binary"), bin.span)
            }
            _ => return,
        };

        if !is_dynamic {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, arg_span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Route path built dynamically (template literal / concatenation) — Elysia AOT can only compile static path strings. Use `:param` segments instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectCtx;
    use crate::rules::test_helpers::run_oxc_ts_with_project;
    use std::path::Path;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_framework(source, &Check, "elysia")
    }


    #[test]
    fn flags_template_literal_with_substitution() {
        let src = "import { Elysia } from 'elysia';\napp.get(`/users/${id}`, () => 'ok');";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_string_concatenation() {
        let src = "import { Elysia } from 'elysia';\napp.post('/users/' + id, () => 'ok');";
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn allows_static_string() {
        let src = "import { Elysia } from 'elysia';\napp.get('/users/:id', () => 'ok');";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_plain_template_string() {
        let src = "import { Elysia } from 'elysia';\napp.get(`/users/:id`, () => 'ok');";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_non_elysia_files() {
        let src = "app.get(`/users/${id}`, () => 'ok');";
        assert!(crate::rules::test_helpers::run_oxc_ts(src, &Check).is_empty());
    }


    #[test]
    fn ignores_fetch_with_template_literal() {
        // Regression: `fetch(`...`)` is a global call, not a route definition.
        // Its callee is an identifier, not a member_expression — already
        // filtered, but we keep this test to lock the behaviour.
        let src = "import { Elysia } from 'elysia';\nconst body = await fetch(`/users/${id}`);";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_file_without_elysia_import() {
        // Regression: a fetch helper in a non-Elysia file with `someClient.get(`/x/${id}`)`.
        let src = "const client = makeClient();\nawait client.get(`/users/${id}`);";
        assert!(run_on(src).is_empty());
    }
}
