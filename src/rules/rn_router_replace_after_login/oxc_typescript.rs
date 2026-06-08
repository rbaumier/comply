//! rn-router-replace-after-login OXC backend.
//!
//! Flags `router.push(...)` inside functions whose name matches
//! `*login* / logout* / signIn* / signOut*` (case-insensitive).

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;

pub struct Check;

fn auth_fn_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("login")
        || lower.starts_with("logout")
        || lower.starts_with("signin")
        || lower.starts_with("signout")
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["router.push"])
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
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "push" {
            return;
        }
        let Expression::Identifier(obj) = &member.object else { return };
        if obj.name.as_str() != "router" {
            return;
        }

        // Walk ancestors to find enclosing function name.
        let Some(fn_name) = enclosing_function_name(node, semantic, ctx.source) else { return };
        if !auth_fn_name(fn_name) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`router.push` inside `{fn_name}` keeps the auth screen on the back stack — use `router.replace`."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn enclosing_function_name<'b>(
    node: &oxc_semantic::AstNode<'b>,
    semantic: &'b oxc_semantic::Semantic<'b>,
    source: &'b str,
) -> Option<&'b str> {
    for ancestor in semantic.nodes().ancestors(node.id()) {
        match ancestor.kind() {
            AstKind::Function(func) => {
                if let Some(ref id) = func.id {
                    return Some(id.name.as_str());
                }
            }
            AstKind::MethodDefinition(method) => {
                let name_span = method.key.span();
                let name = &source[name_span.start as usize..name_span.end as usize];
                return Some(name);
            }
            AstKind::VariableDeclarator(decl) => {
                if let oxc_ast::ast::BindingPattern::BindingIdentifier(ident) = &decl.id {
                    return Some(ident.name.as_str());
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;


    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_push_in_login() {
        let src = "async function handleLogin() { router.push('/home'); }";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_push_in_signout_arrow() {
        let src = "const signOutUser = async () => { router.push('/login'); };";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn allows_replace_in_login() {
        let src = "async function handleLogin() { router.replace('/home'); }";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_push_outside_auth() {
        let src = "function openDetails() { router.push('/details'); }";
        assert!(run(src).is_empty());
    }
}
