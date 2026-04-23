//! xstate-event-names backend — enforce SCREAMING_SNAKE_CASE on event keys
//! inside an `on: { ... }` map.
//!
//! XState convention is that events are named with ALL_CAPS identifiers so
//! they read like discrete signals (`SUBMIT`, `FETCH_DATA`). lowerCamel /
//! kebab-case names typecheck but make transitions hard to scan, and they
//! collide with state-node property conventions.
//!
//! Allowed exceptions:
//! - `*` wildcard handler (catches any event).
//! - `xstate.*` built-in lifecycle events (e.g. `xstate.init`, `xstate.done.*`).

use crate::diagnostic::{Diagnostic, Severity};

/// Strip matching surrounding quote characters from a property key.
fn unquote(s: &str) -> &str {
    s.trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
}

/// True if `obj` is the value of an `on: { ... }` pair.
fn is_on_object(obj: tree_sitter::Node, source: &[u8]) -> bool {
    if obj.kind() != "object" {
        return false;
    }
    let Some(on_pair) = obj.parent() else { return false };
    if on_pair.kind() != "pair" {
        return false;
    }
    let Some(key) = on_pair.child_by_field_name("key") else { return false };
    unquote(key.utf8_text(source).unwrap_or("")) == "on"
}

/// SCREAMING_SNAKE_CASE: starts with A-Z, contains only A-Z, 0-9, underscore.
fn is_screaming_snake(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else { return false };
    if !first.is_ascii_uppercase() {
        return false;
    }
    chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "pair" {
        return;
    }
    let Some(parent_object) = node.parent() else { return };
    if !is_on_object(parent_object, source) {
        return;
    }
    let Some(key_node) = node.child_by_field_name("key") else { return };
    let key_text = unquote(key_node.utf8_text(source).unwrap_or(""));
    if key_text.is_empty() {
        return;
    }
    // Allow wildcard and xstate built-in lifecycle events.
    if key_text == "*" || key_text.starts_with("xstate.") {
        return;
    }
    if is_screaming_snake(key_text) {
        return;
    }

    let pos = key_node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "Event name `{key_text}` should be SCREAMING_SNAKE_CASE (e.g. `FETCH_DATA`)."
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
    fn allows_screaming_snake_events() {
        let src = r#"
            createMachine({
                on: {
                    NEXT: 'b',
                    FETCH_DATA: { target: 'loading' },
                    DONE_2: 'idle',
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_camel_case_event() {
        let src = r#"
            createMachine({
                on: {
                    fetchData: 'loading',
                },
            });
        "#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("fetchData"));
    }

    #[test]
    fn flags_kebab_case_event() {
        let src = r#"
            createMachine({
                on: {
                    'fetch-data': 'loading',
                },
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_wildcard_and_xstate_builtins() {
        let src = r#"
            createMachine({
                on: {
                    '*': 'fallback',
                    'xstate.init': 'starting',
                    'xstate.done.actor.foo': 'ok',
                },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_keys_outside_on() {
        let src = r#"
            const cfg = {
                states: {
                    idle: { entry: 'log' },
                },
            };
        "#;
        assert!(run_on(src).is_empty());
    }
}
