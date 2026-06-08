//! no-unchecked-json-parse OXC backend — flag unwrapped `JSON.parse(...)` calls.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

/// True when `id` resolves to a binding declared with an explicit `unknown`
/// type annotation (e.g. `let body: unknown`). Assigning `JSON.parse()`'s
/// `any` result to an `unknown` target is safe: TypeScript forces every
/// downstream consumer to narrow before use — the same guarantee the rule
/// enforces. A concrete annotation is *not* exempt: `any → T` is a silent
/// unsafe assertion, exactly what the rule should still flag.
fn binding_is_unknown_typed<'a>(
    id: &oxc_ast::ast::IdentifierReference<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let Some(ref_id) = id.reference_id.get() else { return false };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else { return false };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    for kind in std::iter::once(nodes.kind(decl_node_id)).chain(nodes.ancestor_kinds(decl_node_id)) {
        if let AstKind::VariableDeclarator(decl) = kind {
            return is_unknown_annotation(decl.type_annotation.as_deref());
        }
    }
    false
}

/// True when the annotation is exactly `unknown`.
fn is_unknown_annotation(ann: Option<&oxc_ast::ast::TSTypeAnnotation<'_>>) -> bool {
    matches!(
        ann.map(|a| &a.type_annotation),
        Some(oxc_ast::ast::TSType::TSUnknownKeyword(_))
    )
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["JSON"])
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

        // Check if callee is `JSON.parse`.
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        if member.property.name.as_str() != "parse" {
            return;
        }
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "JSON" {
            return;
        }

        // Check if wrapped in a validator: parent is an argument to .parse()/.safeParse().
        let parent = semantic.nodes().parent_node(node.id());
        if let AstKind::CallExpression(outer_call) = parent.kind()
            && let Expression::StaticMemberExpression(outer_member) = &outer_call.callee {
                let method = outer_member.property.name.as_str();
                if method == "parse" || method == "safeParse" {
                    return;
                }
            }

        // `JSON.parse()` assigned to an `unknown`-typed target is safe (#512):
        // the result cannot be used without runtime narrowing.
        match parent.kind() {
            AstKind::AssignmentExpression(assign) => {
                if let oxc_ast::ast::AssignmentTarget::AssignmentTargetIdentifier(id) = &assign.left
                    && binding_is_unknown_typed(id, semantic)
                {
                    return;
                }
            }
            AstKind::VariableDeclarator(decl) => {
                if is_unknown_annotation(decl.type_annotation.as_deref()) {
                    return;
                }
            }
            _ => {}
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`JSON.parse()` returns `any` — wrap it with a Zod schema or type guard before using the result.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::rules::test_helpers::run_oxc_ts;

    #[test]
    fn allows_json_parse_assigned_to_unknown_issue_512() {
        let src = r#"
            let body: unknown = null;
            if (text.length > 0) {
                try {
                    body = JSON.parse(text);
                } catch {
                    body = text;
                }
            }
        "#;
        let diags = run_oxc_ts(src, &Check);
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn allows_json_parse_in_unknown_declarator() {
        let src = "const body: unknown = JSON.parse(text);";
        let diags = run_oxc_ts(src, &Check);
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }

    #[test]
    fn still_flags_unwrapped_json_parse() {
        let src = "const data = JSON.parse(text);";
        let diags = run_oxc_ts(src, &Check);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn still_flags_json_parse_assigned_to_concrete_type() {
        let src = r#"
            let cfg: Config = defaultConfig;
            cfg = JSON.parse(text);
        "#;
        let diags = run_oxc_ts(src, &Check);
        assert_eq!(diags.len(), 1);
    }

    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_bare_variable_declaration() {
        assert_eq!(run("const data = JSON.parse(body);").len(), 1);
    }


    #[test]
    fn flags_return_statement() {
        assert_eq!(
            run("function f(s: string) { return JSON.parse(s); }").len(),
            1
        );
    }


    #[test]
    fn allows_zod_parse_wrapper() {
        assert!(run("const data = schema.parse(JSON.parse(body));").is_empty());
    }


    #[test]
    fn allows_zod_safe_parse_wrapper() {
        assert!(run("const data = schema.safeParse(JSON.parse(body));").is_empty());
    }


    #[test]
    fn flags_bare_return_in_handler() {
        assert_eq!(
            run("function handler(str: string) { return JSON.parse(str); }").len(),
            1
        );
    }
}
