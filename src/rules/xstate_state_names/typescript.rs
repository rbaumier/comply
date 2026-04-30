//! xstate-state-names backend — enforce camelCase or snake_case on state
//! keys inside a `states: { ... }` map.
//!
//! XState convention is that state identifiers are written as lowerCamel or
//! snake_case. PascalCase risks confusion with component names, and
//! SCREAMING_SNAKE is reserved for events by convention.

use crate::diagnostic::{Diagnostic, Severity};

/// Strip matching surrounding quote characters from a property key.
fn unquote(s: &str) -> &str {
    s.trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
}

/// True if `obj` is the value of a `states: { ... }` pair.
fn is_states_object(obj: tree_sitter::Node, source: &[u8]) -> bool {
    if obj.kind() != "object" {
        return false;
    }
    let Some(states_pair) = obj.parent() else {
        return false;
    };
    if states_pair.kind() != "pair" {
        return false;
    }
    let Some(key) = states_pair.child_by_field_name("key") else {
        return false;
    };
    unquote(key.utf8_text(source).unwrap_or("")) == "states"
}

/// camelCase: lowercase first char, alphanumerics only.
fn is_camel_case(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric())
}

/// snake_case: lowercase first char, lowercase letters / digits / underscore only.
fn is_snake_case(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    let Some(parent_object) = node.parent() else { return };
    if !is_states_object(parent_object, source) {
        return;
    }
    let Some(key_node) = node.child_by_field_name("key") else { return };
    let key_text = unquote(key_node.utf8_text(source).unwrap_or(""));
    if key_text.is_empty() {
        return;
    }
    if is_camel_case(key_text) || is_snake_case(key_text) {
        return;
    }

    let pos = key_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "State name `{key_text}` should be camelCase or snake_case (e.g. `fetchingData`)."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn allows_camel_case_state() {
        let src = r#"
            createMachine({
                states: {
                    idle: {},
                    fetchingData: {},
                    ready2go: {},
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_snake_case_state() {
        let src = r#"
            createMachine({
                states: {
                    fetching_data: {},
                    step_1: {},
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_pascal_case_state() {
        let src = r#"
            createMachine({
                states: {
                    Idle: {},
                },
            });
        "#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("Idle"));
    }

    #[test]
    fn flags_screaming_snake_state() {
        let src = r#"
            createMachine({
                states: {
                    IDLE_STATE: {},
                },
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn ignores_keys_outside_states() {
        let src = r#"
            createMachine({
                on: { FETCH: 'loading' },
                context: { Count: 0 },
            });
        "#;
        assert!(run_on(src).is_empty());
    }
}
