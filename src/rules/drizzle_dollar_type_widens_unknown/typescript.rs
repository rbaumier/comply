//! drizzle-dollar-type-widens-unknown — flag `.$type<unknown>()` /
//! `.$type<any>()` calls.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return };
    if prop.utf8_text(source).unwrap_or("") != "$type" {
        return;
    }
    // Type arguments live as a sibling node before the arguments list.
    let mut targs: Option<&str> = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_arguments" {
            targs = Some(child.utf8_text(source).unwrap_or(""));
            break;
        }
    }
    let Some(text) = targs else { return };
    let inner = text.trim_start_matches('<').trim_end_matches('>').trim();
    if inner != "unknown" && inner != "any" {
        return;
    }
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "drizzle-dollar-type-widens-unknown".into(),
        message: format!("`.$type<{}>()` widens the column type away — pass a concrete type instead.", inner),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(src, &Check)
    }

    #[test]
    fn flags_dollar_type_unknown() {
        let src = "const c = json('payload').$type<unknown>();";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_dollar_type_any() {
        let src = "const c = json('payload').$type<any>();";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_dollar_type_concrete() {
        let src = "const c = json('payload').$type<{ a: string }>();";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_dollar_type_named() {
        let src = "const c = json('payload').$type<Payload>();";
        assert!(run(src).is_empty());
    }
}
