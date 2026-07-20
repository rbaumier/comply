//! rn-router-replace-after-login OXC backend.
//!
//! Flags `router.push(...)` inside functions whose name matches
//! `*login* / logout* / signIn* / signOut*` (case-insensitive), when `router` is
//! provably a React-Native / Expo navigation instance.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::{byte_offset_to_line_col, import_source_of, locally_owned_binding_init};
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, IdentifierReference};
use oxc_semantic::Semantic;
use oxc_span::GetSpan;

pub struct Check;

fn auth_fn_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("login")
        || lower.starts_with("logout")
        || lower.starts_with("signin")
        || lower.starts_with("signout")
}

/// A module specifier whose `router`/navigation export carries the
/// `push`/`replace` back-stack semantics this React-Native rule reasons about.
fn is_rn_navigation_module(source: &str) -> bool {
    source == "expo-router"
        || source == "react-native"
        || source.starts_with("@react-navigation/")
}

/// True when the `router` object is provably a React-Native / Expo navigation
/// instance: imported directly from a navigation module (`import { router } from
/// 'expo-router'`) or bound from a `useRouter()` / `useNavigation()` hook that is.
/// A vue-router `Router` (imported from the app's own module) resolves to neither
/// and is not flagged.
fn is_rn_router<'a>(obj: &IdentifierReference, semantic: &'a Semantic<'a>) -> bool {
    if import_source_of(obj, semantic).is_some_and(is_rn_navigation_module) {
        return true;
    }
    let Some(Expression::CallExpression(call)) = locally_owned_binding_init(obj, semantic) else {
        return false;
    };
    let Expression::Identifier(hook) = &call.callee else {
        return false;
    };
    matches!(hook.name.as_str(), "useRouter" | "useNavigation")
        && import_source_of(hook, semantic).is_some_and(is_rn_navigation_module)
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
        if !is_rn_router(obj, semantic) {
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
            severity: Severity::Error,
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

    // Regression for #7478: a vue-router `Router` (imported from the app's own
    // `@/router` module) is not a React-Native navigation object, so
    // `router.push` inside `logout` must not be flagged.
    #[test]
    fn ignores_vue_router_push_in_logout() {
        let src = r#"
            import router from '@/router'
            function logout(redirect = router.currentRoute.value.fullPath) {
                localStorage.removeItem('token')
                router.push({ name: 'login', query: { redirect } }).then(cleanup)
            }
        "#;
        let diags = run_on(src);
        assert!(diags.is_empty(), "{diags:?}");
    }

    // Vue 3's `useRouter()` composable (from `vue-router`) is not a React-Native
    // router, so a `const router = useRouter()` binding must not be flagged.
    #[test]
    fn ignores_vue_use_router_hook_push_in_logout() {
        let src = r#"
            import { useRouter } from 'vue-router'
            function useLogout() {
                const router = useRouter()
                function logout() {
                    router.push({ name: 'login' })
                }
            }
        "#;
        let diags = run_on(src);
        assert!(diags.is_empty(), "{diags:?}");
    }

    // A bare `router` with no resolvable import/hook provenance cannot be proven
    // to be a React-Native router, so it is not flagged.
    #[test]
    fn ignores_router_of_unknown_origin() {
        let src = r#"
            function logout() {
                router.push('/login')
            }
        "#;
        let diags = run_on(src);
        assert!(diags.is_empty(), "{diags:?}");
    }

    // Expo Router `router` singleton imported directly — a genuine positive.
    #[test]
    fn flags_expo_router_import_push_in_logout() {
        let src = r#"
            import { router } from 'expo-router'
            function logout() {
                router.push('/login')
            }
        "#;
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }

    // Expo Router `useRouter()` hook binding — a genuine positive.
    #[test]
    fn flags_expo_use_router_hook_push_in_login() {
        let src = r#"
            import { useRouter } from 'expo-router'
            function LoginScreen() {
                const router = useRouter()
                function onLogin() {
                    router.push('/home')
                }
            }
        "#;
        assert_eq!(run_on(src).len(), 1, "{:?}", run_on(src));
    }
}
