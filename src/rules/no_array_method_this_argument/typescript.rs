//! no-array-method-this-argument backend — flag the `thisArg`
//! parameter in array methods like `.filter(fn, context)`.
//!
//! Why: the `thisArg` parameter is confusing, rarely needed, and
//! doesn't work with arrow functions. Use `.bind()` or a closure.

use crate::diagnostic::{Diagnostic, Severity};

/// Array methods that accept a `thisArg` as their second parameter.
/// (`.reduce()` / `.reduceRight()` take initial-value as 2nd arg, not thisArg.)
const METHODS_WITH_THIS_ARG: &[&str] = &[
    "every",
    "filter",
    "find",
    "findLast",
    "findIndex",
    "findLastIndex",
    "flatMap",
    "forEach",
    "map",
    "some",
];

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    // Must be a member expression call: `something.method(callback, thisArg)`
    let Some(function) = node.child_by_field_name("function") else {
        return;
    };
    if function.kind() != "member_expression" {
        return;
    }

    // Extract the method name
    let Some(property) = function.child_by_field_name("property") else {
        return;
    };
    let Ok(method_name) = property.utf8_text(source) else {
        return;
    };

    if !METHODS_WITH_THIS_ARG.contains(&method_name) {
        return;
    }

    // Get the arguments node
    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };

    // Check that there are exactly 2 named children (callback + thisArg)
    if args.named_child_count() != 2 {
        return;
    }

    // The second argument is the `thisArg` — flag it
    let Some(this_arg) = args.named_child(1) else {
        return;
    };

    let pos = this_arg.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-array-method-this-argument".into(),
        message: format!(
            "Do not use the `this` argument in `Array#{}()` — use `.bind()` or an arrow function instead.",
            method_name
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
    fn flags_filter_with_this_arg() {
        assert_eq!(run_on("arr.filter(fn, context);").len(), 1);
    }

    #[test]
    fn flags_map_with_this_arg() {
        assert_eq!(run_on("arr.map(fn, thisObj);").len(), 1);
    }

    #[test]
    fn flags_every_with_this_arg() {
        assert_eq!(run_on("arr.every(fn, ctx);").len(), 1);
    }

    #[test]
    fn allows_filter_without_this_arg() {
        assert!(run_on("arr.filter(x => x > 0);").is_empty());
    }

    #[test]
    fn allows_reduce_with_initial_value() {
        // reduce's 2nd arg is initial value, not thisArg
        assert!(run_on("arr.reduce((acc, x) => acc + x, 0);").is_empty());
    }

    #[test]
    fn allows_non_array_method() {
        assert!(run_on("foo.bar(fn, context);").is_empty());
    }
}
