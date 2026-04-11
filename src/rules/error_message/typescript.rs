//! error-message backend — flag `new Error()` without a message argument.

use crate::diagnostic::{Diagnostic, Severity};

/// Built-in error constructors and the index of their message argument.
/// Most take message at index 0; AggregateError at 1, SuppressedError at 2.
const BUILTIN_ERRORS: &[&str] = &[
    "Error",
    "EvalError",
    "RangeError",
    "ReferenceError",
    "SyntaxError",
    "TypeError",
    "URIError",
    "AggregateError",
    "SuppressedError",
];

fn message_arg_index(ctor_name: &str) -> usize {
    match ctor_name {
        "AggregateError" => 1,
        "SuppressedError" => 2,
        _ => 0,
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    // Match both `new Error()` and `Error()` call forms.
    let (ctor_node, args_node) = match node.kind() {
        "new_expression" => {
            let ctor = match node.child_by_field_name("constructor") {
                Some(c) => c,
                None => return,
            };
            let args = match node.child_by_field_name("arguments") {
                Some(a) => a,
                None => {
                    // `new Error` without parens — no arguments at all.
                    let ctor_name = ctor.utf8_text(source).unwrap_or("");
                    if !BUILTIN_ERRORS.contains(&ctor_name) { return; }
                    let pos = node.start_position();
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: pos.row + 1,
                        column: pos.column + 1,
                        rule_id: "error-message".into(),
                        message: format!(
                            "Pass a message to the `{ctor_name}` constructor."
                        ),
                        severity: Severity::Warning,
                    });
                    return;
                }
            };
            (ctor, args)
        }
        "call_expression" => {
            let func = match node.child_by_field_name("function") {
                Some(f) => f,
                None => return,
            };
            let args = match node.child_by_field_name("arguments") {
                Some(a) => a,
                None => return,
            };
            (func, args)
        }
        _ => return,
    };

    let ctor_name = ctor_node.utf8_text(source).unwrap_or("");
    if !BUILTIN_ERRORS.contains(&ctor_name) {
        return;
    }

    let msg_index = message_arg_index(ctor_name);

    // Collect actual argument nodes (skip parens, commas).
    let mut args = Vec::new();
    let mut has_spread_before_msg = false;
    let count = args_node.named_child_count();
    for i in 0..count {
        let child = match args_node.named_child(i) {
            Some(c) => c,
            None => continue,
        };
        if child.kind() == "spread_element" && args.len() <= msg_index {
            has_spread_before_msg = true;
        }
        args.push(child);
    }

    // If there's a spread element at or before the message index, bail — we
    // can't statically determine the message argument.
    if has_spread_before_msg {
        return;
    }

    let msg_node = args.get(msg_index);

    match msg_node {
        None => {
            // No message argument provided.
            let pos = node.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "error-message".into(),
                message: format!(
                    "Pass a message to the `{ctor_name}` constructor."
                ),
                severity: Severity::Warning,
            });
        }
        Some(arg) => {
            let kind = arg.kind();
            let text = arg.utf8_text(source).unwrap_or("");

            // Array or object literal — not a string.
            if kind == "array" || kind == "object" {
                let pos = arg.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "error-message".into(),
                    message: "Error message should be a string.".into(),
                    severity: Severity::Warning,
                });
                return;
            }

            // Empty string literal: "" or ''
            if kind == "string" && (text == "\"\"" || text == "''") {
                let pos = arg.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "error-message".into(),
                    message: "Error message should not be an empty string.".into(),
                    severity: Severity::Warning,
                });
                return;
            }

            // Empty template string: ``
            if kind == "template_string" && text == "``" {
                let pos = arg.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "error-message".into(),
                    message: "Error message should not be an empty string.".into(),
                    severity: Severity::Warning,
                });
                return;
            }

            // Numeric or boolean literal — not a string.
            if kind == "number" || kind == "true" || kind == "false" {
                let pos = arg.start_position();
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: pos.row + 1,
                    column: pos.column + 1,
                    rule_id: "error-message".into(),
                    message: "Error message should be a string.".into(),
                    severity: Severity::Warning,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_new_error_without_message() {
        let d = run_on("throw new Error();");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Pass a message"));
    }

    #[test]
    fn flags_empty_string_message() {
        let d = run_on("throw new Error('');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("empty string"));
    }

    #[test]
    fn flags_non_string_message() {
        let d = run_on("throw new TypeError(123);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("should be a string"));
    }

    #[test]
    fn flags_array_message() {
        let d = run_on("throw new Error([]);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("should be a string"));
    }

    #[test]
    fn flags_object_message() {
        let d = run_on("throw new Error({});");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("should be a string"));
    }

    #[test]
    fn allows_string_message() {
        assert!(run_on("throw new Error('Something went wrong');").is_empty());
    }

    #[test]
    fn allows_template_string_message() {
        assert!(run_on("throw new Error(`Expected ${type}`);").is_empty());
    }

    #[test]
    fn allows_variable_message() {
        assert!(run_on("throw new Error(message);").is_empty());
    }

    #[test]
    fn flags_aggregate_error_without_message() {
        // AggregateError(errors, message) — message is at index 1
        let d = run_on("throw new AggregateError(errors);");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_aggregate_error_with_message() {
        assert!(run_on("throw new AggregateError(errors, 'msg');").is_empty());
    }

    #[test]
    fn allows_spread_arguments() {
        assert!(run_on("throw new Error(...args);").is_empty());
    }

    #[test]
    fn flags_call_without_new() {
        let d = run_on("throw Error();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_non_error_constructor() {
        assert!(run_on("new MyClass();").is_empty());
    }
}
