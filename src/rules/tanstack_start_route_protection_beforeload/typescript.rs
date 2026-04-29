//! Flag a `useEffect` callback that contains a redirect to an auth-looking
//! route (`/login`, `/signin`, `/auth`, etc.) via `navigate(...)`,
//! `router.push(...)`, `router.replace(...)`, `redirect(...)`, or
//! `window.location` assignments. The correct TanStack Start approach is
//! `beforeLoad` + `throw redirect()`.

use crate::diagnostic::{Diagnostic, Severity};

const AUTH_PATH_MARKERS: &[&str] = &["login", "signin", "sign-in", "auth", "authenticate"];

crate::ast_check! { on ["call_expression"] prefilter = ["beforeLoad"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return; };
    let Ok(name) = callee.utf8_text(source) else { return; };
    if name != "useEffect" { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(cb) = first_function_argument(args) else { return; };
    if !contains_auth_redirect(cb, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Don't redirect to an auth route from `useEffect`. Move the guard to \
         `beforeLoad` and `throw redirect({ to: '/login' })` on the route."
            .into(),
        Severity::Warning,
    ));
}

fn first_function_argument<'a>(args: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
    let mut cursor = args.walk();
    args.children(&mut cursor)
        .find(|c| matches!(c.kind(), "arrow_function" | "function_expression" | "function"))
}

fn contains_auth_redirect(root: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        // Call-form auth redirects: navigate(...), router.push(...),
        // router.replace(...), redirect(...).
        if n.kind() == "call_expression"
            && let Some(callee) = n.child_by_field_name("function")
            && let Ok(name) = callee.utf8_text(source)
            && is_redirect_callee(name)
            && let Some(args) = n.child_by_field_name("arguments")
            && arg_targets_auth(args, source)
        {
            return true;
        }

        // window.location = '...' / window.location.href = '...' /
        // window.location.assign('...') / .replace('...').
        if n.kind() == "assignment_expression"
            && let Some(left) = n.child_by_field_name("left")
            && let Ok(lt) = left.utf8_text(source)
            && (lt == "window.location" || lt == "window.location.href")
            && let Some(right) = n.child_by_field_name("right")
            && expr_is_auth_string(right, source)
        {
            return true;
        }
        if n.kind() == "call_expression"
            && let Some(callee) = n.child_by_field_name("function")
            && callee.kind() == "member_expression"
            && let Ok(callee_text) = callee.utf8_text(source)
            && (callee_text == "window.location.assign"
                || callee_text == "window.location.replace")
            && let Some(args) = n.child_by_field_name("arguments")
            && arg_targets_auth(args, source)
        {
            return true;
        }

        let mut cursor = n.walk();
        for c in n.children(&mut cursor) {
            stack.push(c);
        }
    }
    false
}

fn is_redirect_callee(name: &str) -> bool {
    if name == "navigate" || name == "redirect" {
        return true;
    }
    if name.ends_with(".navigate") || name.ends_with(".push") || name.ends_with(".replace") {
        return true;
    }
    false
}

fn path_looks_like_auth(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    AUTH_PATH_MARKERS.iter().any(|m| lower.contains(m))
}

fn expr_is_auth_string(n: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if !matches!(n.kind(), "string" | "template_string") { return false; }
    let Ok(t) = n.utf8_text(source) else { return false; };
    let inner = t.trim_matches(|ch| ch == '"' || ch == '\'' || ch == '`');
    path_looks_like_auth(inner)
}

fn arg_targets_auth(args: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut cursor = args.walk();
    for c in args.children(&mut cursor) {
        match c.kind() {
            "string" | "template_string" => {
                if expr_is_auth_string(c, source) { return true; }
            }
            "object" => {
                // { to: '/login' } / { to: '/auth/sign-in' }
                let mut cur = c.walk();
                for pair in c.children(&mut cur) {
                    if pair.kind() != "pair" { continue; }
                    let Some(k) = pair.child_by_field_name("key") else { continue; };
                    let Ok(kt) = k.utf8_text(source) else { continue; };
                    let kname = kt.trim_matches(|ch| ch == '"' || ch == '\'');
                    if kname != "to" { continue; }
                    let Some(v) = pair.child_by_field_name("value") else { continue; };
                    if expr_is_auth_string(v, source) { return true; }
                }
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }

    #[test]
    fn flags_effect_navigate_string() {
        let src = "useEffect(() => { if (!user) navigate('/login'); }, [user]);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_effect_navigate_object() {
        let src = "useEffect(() => { navigate({ to: '/login' }); }, []);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_effect_without_login_redirect() {
        let src = "useEffect(() => { doThing(); }, []);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_navigate_to_other_route() {
        let src = "useEffect(() => { navigate('/dashboard'); }, []);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_router_push_to_signin() {
        let src = "useEffect(() => { router.push('/signin'); }, []);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_router_replace_to_auth() {
        let src = "useEffect(() => { router.replace('/auth/callback'); }, []);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_redirect_to_signin_dash() {
        let src = "useEffect(() => { redirect('/sign-in'); }, []);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_window_location_assignment() {
        let src = "useEffect(() => { window.location.href = '/login'; }, []);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_window_location_assign_call() {
        let src = "useEffect(() => { window.location.assign('/login'); }, []);";
        assert_eq!(run(src).len(), 1);
    }
}
