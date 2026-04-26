//! Flags `router.push(...)` inside functions whose name matches
//! `*login* / logout* / signIn* / signOut*` (case-insensitive).

use crate::diagnostic::{Diagnostic, Severity};

fn auth_fn_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("login")
        || lower.starts_with("logout")
        || lower.starts_with("signin")
        || lower.starts_with("signout")
}

fn enclosing_function_name<'a>(node: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let mut current = node.parent();
    while let Some(n) = current {
        match n.kind() {
            "function_declaration" | "generator_function_declaration" => {
                let name = n.child_by_field_name("name")?;
                return name.utf8_text(source).ok();
            }
            "method_definition" => {
                let name = n.child_by_field_name("name")?;
                return name.utf8_text(source).ok();
            }
            "variable_declarator" => {
                // const foo = () => ...  or  const foo = async () => ...
                let name = n.child_by_field_name("name")?;
                return name.utf8_text(source).ok();
            }
            _ => {}
        }
        current = n.parent();
    }
    None
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }
    let Some(obj) = func.child_by_field_name("object") else { return };
    let Ok(obj_text) = obj.utf8_text(source) else { return };
    if obj_text != "router" { return; }
    let Some(prop) = func.child_by_field_name("property") else { return };
    let Ok(prop_name) = prop.utf8_text(source) else { return };
    if prop_name != "push" { return; }
    let Some(fn_name) = enclosing_function_name(node, source) else { return };
    if !auth_fn_name(fn_name) { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("`router.push` inside `{fn_name}` keeps the auth screen on the back stack — use `router.replace`."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(s, &Check)
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
