//! xstate-no-invalid-conditional-action — flag `choose(...)` calls whose
//! branch objects are missing a `guard`/`cond` predicate or an `actions`
//! list. Both properties are required: without `guard`/`cond`, the branch
//! is unconditional (likely a bug); without `actions`, the branch has no
//! effect.

use crate::diagnostic::{Diagnostic, Severity};

/// Return the property key string for a `pair` node, stripped of quotes.
fn pair_key<'a>(pair: tree_sitter::Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let key = pair.child_by_field_name("key")?;
    let text = key.utf8_text(source).ok()?;
    Some(text.trim_matches(|c: char| c == '\'' || c == '"' || c == '`'))
}

/// Given an `object` node, check whether it contains a property with any of
/// `names` as its key.
fn object_has_key(obj: tree_sitter::Node, source: &[u8], names: &[&str]) -> bool {
    let mut cursor = obj.walk();
    for child in obj.named_children(&mut cursor) {
        if child.kind() != "pair" {
            continue;
        }
        if let Some(k) = pair_key(child, source)
            && names.contains(&k)
        {
            return true;
        }
    }
    false
}

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "identifier" {
        return;
    }
    if callee.utf8_text(source).unwrap_or("") != "choose" {
        return;
    }

    let Some(args) = node.child_by_field_name("arguments") else { return };
    // First argument should be an array literal of branch objects.
    let mut arg_cursor = args.walk();
    let first_array = args
        .named_children(&mut arg_cursor)
        .find(|c| c.kind() == "array");
    let Some(array) = first_array else { return };

    let mut branch_cursor = array.walk();
    for element in array.named_children(&mut branch_cursor) {
        if element.kind() != "object" {
            continue;
        }

        let has_guard = object_has_key(element, source, &["guard", "cond"]);
        let has_actions = object_has_key(element, source, &["actions"]);

        if has_guard && has_actions {
            continue;
        }

        let message = match (has_guard, has_actions) {
            (false, false) => {
                "`choose()` branch is missing both `guard`/`cond` and `actions`.".to_string()
            }
            (false, true) => {
                "`choose()` branch is missing `guard`/`cond`.".to_string()
            }
            (true, false) => {
                "`choose()` branch is missing `actions`.".to_string()
            }
            (true, true) => unreachable!(),
        };

        diagnostics.push(Diagnostic::at_node(
            ctx.path,
            &element,
            super::META.id,
            message,
            Severity::Warning,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_branch_missing_guard() {
        let src = r#"
            choose([
                { actions: ['doThing'] },
            ]);
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_branch_missing_actions() {
        let src = r#"
            choose([
                { guard: 'isAllowed' },
            ]);
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_branch_missing_both() {
        let src = r#"
            choose([
                { target: 'next' },
            ]);
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_valid_branch_with_guard_and_actions() {
        let src = r#"
            choose([
                { guard: 'isAllowed', actions: ['doThing'] },
            ]);
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_valid_branch_with_cond_and_actions() {
        let src = r#"
            choose([
                { cond: 'isAllowed', actions: ['doThing'] },
            ]);
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_only_invalid_branch_among_valid_ones() {
        let src = r#"
            choose([
                { guard: 'a', actions: ['x'] },
                { actions: ['y'] },
                { cond: 'b', actions: ['z'] },
            ]);
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn ignores_non_choose_calls() {
        let src = r#"
            other([
                { foo: 'bar' },
            ]);
        "#;
        assert!(run_on(src).is_empty());
    }
}
