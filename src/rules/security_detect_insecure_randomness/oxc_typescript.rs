//! security-detect-insecure-randomness oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Security-context identifier names — when the result of
/// `Math.random()` flows into one of these, it's almost certainly a
/// real cryptographic-security mistake.
const SECURITY_CONTEXT_NAMES: &[&str] = &[
    "token",
    "tokens",
    "secret",
    "password",
    "passwd",
    "passphrase",
    "session",
    "sessionId",
    "sessionToken",
    "apiKey",
    "key",
    "salt",
    "nonce",
    "id",
    "uuid",
    "csrf",
    "csrfToken",
    "resetCode",
    "verification",
    "verificationCode",
    "otp",
];

/// Vue render functions from the `vue` package. In their props/attrs object,
/// `key` is Vue's reserved list-diff identity hint and `id` is a pass-through
/// DOM attribute — neither is a cryptographic key. Recognised only when the
/// callee resolves to a `vue` import, never by bare name.
const VUE_RENDER_FUNCTIONS: &[&str] =
    &["h", "createVNode", "createElementVNode", "createElementBlock"];

/// True when `prop_id` (an `ObjectProperty`) belongs to an object literal passed
/// directly as an argument to a `vue` render call — `h(Comp, { key, ... })`. The
/// `ObjectProperty` → `ObjectExpression` → `CallExpression` parent chain plus an
/// import-anchored callee identity is a structural check; a bare local `h` is
/// never trusted and stays a security sink.
fn is_vue_render_reserved_key(
    prop_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
) -> bool {
    let nodes = semantic.nodes();
    let object_id = nodes.parent_id(prop_id);
    if !matches!(nodes.get_node(object_id).kind(), AstKind::ObjectExpression(_)) {
        return false;
    }
    let AstKind::CallExpression(call) = nodes.get_node(nodes.parent_id(object_id)).kind() else {
        return false;
    };
    let Expression::Identifier(callee) = &call.callee else {
        return false;
    };
    VUE_RENDER_FUNCTIONS.contains(&callee.name.as_str())
        && crate::oxc_helpers::is_imported_from_vue(callee.name.as_str(), semantic)
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["Math.random"])
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
        let Expression::StaticMemberExpression(member) = &call.callee else {
            return;
        };
        let Expression::Identifier(obj) = &member.object else {
            return;
        };
        if obj.name.as_str() != "Math" || member.property.name.as_str() != "random" {
            return;
        }
        // Walk up to find the enclosing VariableDeclarator / FormalParameter /
        // ReturnStatement and check if the surrounding identifier matches a
        // security context.
        let mut current_id = node.id();
        let mut hit_security_context = false;
        for _ in 0..6 {
            let parent_id = semantic.nodes().parent_id(current_id);
            if parent_id == current_id {
                break;
            }
            let parent = semantic.nodes().get_node(parent_id);
            match parent.kind() {
                AstKind::VariableDeclarator(decl) => {
                    if let oxc_ast::ast::BindingPattern::BindingIdentifier(id) = &decl.id
                        && SECURITY_CONTEXT_NAMES.contains(&id.name.as_str())
                    {
                        hit_security_context = true;
                    }
                    break;
                }
                AstKind::ObjectProperty(p) => {
                    if let oxc_ast::ast::PropertyKey::StaticIdentifier(key) = &p.key
                        && SECURITY_CONTEXT_NAMES.contains(&key.name.as_str())
                        && !(matches!(key.name.as_str(), "key" | "id")
                            && is_vue_render_reserved_key(parent_id, semantic))
                    {
                        hit_security_context = true;
                    }
                    break;
                }
                _ => {}
            }
            current_id = parent_id;
        }
        if !hit_security_context {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`Math.random()` is not cryptographically secure — flowing it \
                      into a token / secret / session id is exploitable. Use \
                      `crypto.randomUUID()` / `crypto.getRandomValues(...)` instead."
                .into(),
            severity: Severity::Error,
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

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    fn run_tsx(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "t.tsx")
    }

    #[test]
    fn flags_math_random_into_token() {
        let src = r#"const token = Math.random().toString(36);"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_math_random_into_sessionid() {
        let src = r#"const sessionId = Math.random();"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_math_random_into_apikey() {
        let src = r#"const apiKey = Math.random();"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_math_random_for_non_security_context() {
        let src = r#"const delay = Math.random() * 1000;"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_vnode_key_in_vue_h_render() {
        let src = r#"
            import { h } from 'vue';
            const vnode = h(MkUrl, { key: Math.random(), url: token.props.url });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_vnode_id_in_vue_create_vnode_render() {
        let src = r#"
            import { createVNode } from 'vue';
            const vnode = createVNode(MkUrl, { id: Math.random() });
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn ignores_jsx_key_attribute() {
        let src = r#"const el = <Foo key={Math.random()} />;"#;
        assert!(run_tsx(src).is_empty());
    }

    #[test]
    fn flags_bare_key_in_plain_object() {
        let src = r#"const cfg = { key: Math.random() };"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_token_in_plain_object() {
        let src = r#"const o = { token: Math.random() };"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_compound_sink_in_vue_h_render() {
        let src = r#"
            import { h } from 'vue';
            const vnode = h(MkUrl, { token: Math.random() });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_vnode_key_when_h_is_local_not_vue() {
        let src = r#"
            function h(_c: unknown, props: unknown) { return props; }
            const x = h(MkUrl, { key: Math.random() });
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
