//! Flags `navigation.navigate('RouteName', ...)` calls with a string-literal first arg.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" { return; }
    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }
    let Some(prop) = func.child_by_field_name("property") else { return };
    let Ok(prop_name) = prop.utf8_text(source) else { return };
    if prop_name != "navigate" { return; }
    let Some(obj) = func.child_by_field_name("object") else { return };
    let Ok(obj_text) = obj.utf8_text(source) else { return };
    if !obj_text.ends_with("navigation") { return; }
    let Some(args) = node.child_by_field_name("arguments") else { return };
    let Some(first) = args.named_child(0) else { return };
    if first.kind() != "string" { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`navigation.navigate('Name', ...)` uses an untyped string route — use `router.push('/path')` from expo-router.".into(),
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
    fn flags_navigate_string() {
        let src = "navigation.navigate('Home', { id: 1 });";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_router_push() {
        let src = "router.push('/home');";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_typed_object_arg() {
        let src = "navigation.navigate({ screen: 'Home' });";
        assert!(run(src).is_empty());
    }
}
