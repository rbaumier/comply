//! xstate-entry-exit-action backend — validate that the value of an `entry`
//! or `exit` property on a state node is a string, function/action creator
//! call, identifier (action name reference), or array of those.
//!
//! A plain object literal as the value (e.g. `entry: { type: 'log' }`) is
//! almost always a mistake — XState expects an action descriptor produced by
//! `assign(...)`, `send(...)`, `raise(...)`, etc., or a string action name.

use crate::diagnostic::{Diagnostic, Severity};

/// Strip matching surrounding quote characters from a property key.
fn unquote(s: &str) -> &str {
    s.trim_matches(|c: char| c == '"' || c == '\'' || c == '`')
}

/// Accepted value shapes for `entry` / `exit`:
/// - string / template_string: action name.
/// - identifier: reference to an action.
/// - call_expression: `assign(...)`, `send(...)`, etc.
/// - arrow_function / function_expression: inline action.
/// - array: list of any of the above (not individually validated here — an
///   array literal is accepted as-is so the simpler rule stays cheap).
fn is_valid_action_value(kind: &str) -> bool {
    matches!(
        kind,
        "string"
            | "template_string"
            | "identifier"
            | "call_expression"
            | "arrow_function"
            | "function_expression"
            | "array"
    )
}

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    let Some(key_node) = node.child_by_field_name("key") else { return };
    let key = unquote(key_node.utf8_text(source).unwrap_or(""));
    if key != "entry" && key != "exit" {
        return;
    }
    let Some(value_node) = node.child_by_field_name("value") else { return };
    if is_valid_action_value(value_node.kind()) {
        return;
    }

    let pos = value_node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: super::META.id.into(),
        message: format!(
            "`{key}` must be a string, function, action creator call, or array — got `{}`.",
            value_node.kind()
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
    fn allows_string_entry() {
        let src = r#"
            createMachine({
                states: { idle: { entry: 'log' } },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_array_entry() {
        let src = r#"
            createMachine({
                states: { idle: { entry: ['log', 'notify'] } },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_call_entry() {
        let src = r#"
            createMachine({
                states: { idle: { entry: assign({ count: 0 }) } },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_arrow_exit() {
        let src = r#"
            createMachine({
                states: { idle: { exit: () => console.log('bye') } },
            });
        "#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_object_entry() {
        let src = r#"
            createMachine({
                states: { idle: { entry: { type: 'log' } } },
            });
        "#;
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("entry"));
    }

    #[test]
    fn flags_number_exit() {
        let src = r#"
            createMachine({
                states: { idle: { exit: 42 } },
            });
        "#;
        assert_eq!(run_on(src).len(), 1);
    }
}
