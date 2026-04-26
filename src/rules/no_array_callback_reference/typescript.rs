//! no-array-callback-reference backend — flag passing a function
//! reference directly to an iterator method like `.map(parseInt)`.
//!
//! Why: iterator methods pass extra arguments (element, index, array)
//! to the callback. `parseInt("11", 2, ...)` is radix 2, not 10.
//! Always wrap: `.map(x => parseInt(x))`.

use crate::diagnostic::{Diagnostic, Severity};

/// Iterator methods that take a callback as first argument.
const ITERATOR_METHODS: &[&str] = &[
    "every",
    "filter",
    "find",
    "findLast",
    "findIndex",
    "findLastIndex",
    "flatMap",
    "forEach",
    "map",
    "reduce",
    "reduceRight",
    "some",
];

/// Built-in constructors that are safe to pass directly (e.g. `Boolean`).
const IGNORED_IDENTIFIERS: &[&str] = &["Boolean", "String", "Number", "BigInt", "Symbol"];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // Must be a member expression call: `something.method(callback)`
    let Some(function) = node.child_by_field_name("function") else {
        return;
    };
    if function.kind() != "member_expression" {
        return;
    }

    // Extract the method name (the `property` field of the member_expression)
    let Some(property) = function.child_by_field_name("property") else {
        return;
    };
    let Ok(method_name) = property.utf8_text(source) else {
        return;
    };

    if !ITERATOR_METHODS.contains(&method_name) {
        return;
    }

    // Get the arguments node
    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };

    // Get the first argument (the callback)
    let Some(first_arg) = args.named_child(0) else {
        return;
    };

    // Only flag identifiers and member_expressions (function references).
    // Skip arrow functions, function expressions, and call expressions — those are fine.
    match first_arg.kind() {
        "identifier" => {
            let Ok(name) = first_arg.utf8_text(source) else {
                return;
            };
            // Allow safe built-in constructors
            if IGNORED_IDENTIFIERS.contains(&name) {
                return;
            }
            let pos = first_arg.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-array-callback-reference".into(),
                message: format!(
                    "Do not pass function `{}` directly to `.{}(…)` — use `(…) => {}(…)` instead.",
                    name, method_name, name
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        "member_expression" => {
            let Ok(text) = first_arg.utf8_text(source) else {
                return;
            };
            let pos = first_arg.start_position();
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: pos.row + 1,
                column: pos.column + 1,
                rule_id: "no-array-callback-reference".into(),
                message: format!(
                    "Do not pass `{}` directly to `.{}(…)` — wrap it in an arrow function.",
                    text, method_name
                ),
                severity: Severity::Warning,
                span: None,
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_map_with_parse_int() {
        assert_eq!(run_on("const x = arr.map(parseInt);").len(), 1);
    }

    #[test]
    fn flags_filter_with_identifier() {
        assert_eq!(run_on("const x = arr.filter(myFunc);").len(), 1);
    }

    #[test]
    fn flags_map_with_member_expression() {
        assert_eq!(run_on("const x = arr.map(utils.transform);").len(), 1);
    }

    #[test]
    fn allows_arrow_function() {
        assert!(run_on("const x = arr.map(x => parseInt(x));").is_empty());
    }

    #[test]
    fn allows_function_expression() {
        assert!(run_on("const x = arr.map(function(x) { return x * 2; });").is_empty());
    }

    #[test]
    fn allows_boolean_constructor() {
        assert!(run_on("const x = arr.filter(Boolean);").is_empty());
    }

    #[test]
    fn allows_non_iterator_method() {
        assert!(run_on("const x = foo.bar(parseInt);").is_empty());
    }
}
