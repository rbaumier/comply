//! Flag a `useEffect` callback that contains `navigate('/login')` (or any
//! string arg starting with `/login`). That pattern redirects after render
//! — in TanStack Start the correct approach is `beforeLoad` + `throw redirect()`.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let Some(callee) = node.child_by_field_name("function") else { return; };
    let Ok(name) = callee.utf8_text(source) else { return; };
    if name != "useEffect" { return; }

    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let Some(cb) = first_function_argument(args) else { return; };
    if !contains_login_navigate(cb, source) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Don't redirect to `/login` from `useEffect`. Move the guard to \
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

fn contains_login_navigate(root: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        if n.kind() == "call_expression"
            && let Some(callee) = n.child_by_field_name("function")
                && let Ok(name) = callee.utf8_text(source)
                    && (name == "navigate" || name.ends_with(".navigate"))
                        && let Some(args) = n.child_by_field_name("arguments")
                            && arg_targets_login(args, source) {
                                return true;
                            }
        let mut cursor = n.walk();
        for c in n.children(&mut cursor) {
            stack.push(c);
        }
    }
    false
}

fn arg_targets_login(args: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut cursor = args.walk();
    for c in args.children(&mut cursor) {
        match c.kind() {
            "string" | "template_string" => {
                if let Ok(t) = c.utf8_text(source) {
                    let inner = t.trim_matches(|ch| ch == '"' || ch == '\'' || ch == '`');
                    if inner.starts_with("/login") {
                        return true;
                    }
                }
            }
            "object" => {
                // navigate({ to: '/login' })
                let mut cur = c.walk();
                for pair in c.children(&mut cur) {
                    if pair.kind() != "pair" { continue; }
                    let Some(k) = pair.child_by_field_name("key") else { continue; };
                    let Ok(kt) = k.utf8_text(source) else { continue; };
                    let kname = kt.trim_matches(|ch| ch == '"' || ch == '\'');
                    if kname != "to" { continue; }
                    let Some(v) = pair.child_by_field_name("value") else { continue; };
                    if !matches!(v.kind(), "string" | "template_string") { continue; }
                    if let Ok(vt) = v.utf8_text(source) {
                        let inner = vt.trim_matches(|ch| ch == '"' || ch == '\'' || ch == '`');
                        if inner.starts_with("/login") {
                            return true;
                        }
                    }
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
}
