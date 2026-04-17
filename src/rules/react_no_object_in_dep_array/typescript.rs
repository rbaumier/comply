//! react-no-object-in-dep-array backend for TypeScript / TSX.
//!
//! Flags dep array entries in `useEffect` / `useMemo` / `useCallback` that
//! are AST-detectably a new allocation on every render:
//!
//! - Object literals: `{ foo: 1 }`.
//! - Array literals: `[1, 2, 3]`.
//! - Spreads whose argument is an object/array literal: `{ ...{a:1} }`, `[...[1]]`.
//! - Inline arrow / function expressions: `() => x`, `function () {}`.
//! - `new Map()` / `new Set()` / `new Date()` / `new WeakMap()` / `new WeakSet()`
//!   / `new Error()` — constructors that obviously allocate.
//! - `Object.assign(...)` and `Object.create(...)` calls.
//!
//! Bare identifiers, member expressions, literals, template literals,
//! conditional expressions, binary expressions, and other call expressions
//! are left alone — the rule can't statically know whether they yield a
//! stable reference, so it refuses to guess.

use crate::diagnostic::{Diagnostic, Severity};

const HOOKS: &[&str] = &["useEffect", "useMemo", "useCallback"];

/// Constructors that always allocate a fresh object.
const ALLOCATING_CONSTRUCTORS: &[&str] = &[
    "Map", "Set", "WeakMap", "WeakSet", "Date", "Error", "Array", "Object",
    "RegExp", "Promise",
];

/// Member-call callees that always return a new object/array.
const ALLOCATING_MEMBER_CALLS: &[(&str, &str)] =
    &[("Object", "assign"), ("Object", "create")];

fn label_for(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "object" => Some("Object literal".to_string()),
        "array" => Some("Array literal".to_string()),
        "arrow_function" => Some("Inline arrow function".to_string()),
        "function_expression" | "function" => {
            Some("Inline function expression".to_string())
        }
        "spread_element" => {
            // Flag only if the spread argument is an object or array literal.
            let arg = node.named_child(0)?;
            match arg.kind() {
                "object" => Some("Spread of object literal".to_string()),
                "array" => Some("Spread of array literal".to_string()),
                _ => None,
            }
        }
        "new_expression" => {
            let ctor = node.child_by_field_name("constructor")?;
            let name = ctor.utf8_text(source).ok()?;
            if ALLOCATING_CONSTRUCTORS.contains(&name) {
                Some(format!("`new {name}()`"))
            } else {
                None
            }
        }
        "call_expression" => {
            let callee = node.child_by_field_name("function")?;
            if callee.kind() != "member_expression" {
                return None;
            }
            let obj = callee.child_by_field_name("object")?;
            let prop = callee.child_by_field_name("property")?;
            let obj_name = obj.utf8_text(source).ok()?;
            let prop_name = prop.utf8_text(source).ok()?;
            if ALLOCATING_MEMBER_CALLS.contains(&(obj_name, prop_name)) {
                Some(format!("`{obj_name}.{prop_name}()` call"))
            } else {
                None
            }
        }
        _ => None,
    }
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some(fn_name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if !HOOKS.contains(&fn_name) {
        return;
    }
    let Some(args) = node.child_by_field_name("arguments") else {
        return;
    };
    let arg_count = args.named_child_count();
    if arg_count < 2 {
        return; // No dep array (callback only).
    }
    let Some(last_arg) = args.named_child(arg_count - 1) else {
        return;
    };
    if last_arg.kind() != "array" {
        return;
    }
    let mut dep_cursor = last_arg.walk();
    for dep in last_arg.named_children(&mut dep_cursor) {
        let Some(label) = label_for(dep, source) else {
            continue;
        };
        let pos = dep.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-no-object-in-dep-array".into(),
            message: format!(
                "{label} in `{fn_name}` dep array — creates a fresh reference \
                 every render. Extract to a memoized value or depend on \
                 primitive fields instead."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    // --- Positive cases: must flag ---

    #[test]
    fn flags_inline_object_literal() {
        let source = "useCallback(fn, [{}]);";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_inline_object_literal_with_fields() {
        let source = "useMemo(() => x, [{ foo: 1 }]);";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_inline_array_literal() {
        let source = "useCallback(fn, [[1, 2]]);";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_new_map() {
        let source = "useCallback(fn, [new Map()]);";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_new_set() {
        let source = "useEffect(() => {}, [new Set([1, 2])]);";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_new_date() {
        let source = "useMemo(() => x, [new Date()]);";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_inline_arrow() {
        let source = "useCallback(fn, [() => x]);";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_inline_function_expression() {
        let source = "useCallback(fn, [function () { return x; }]);";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_object_assign_call() {
        let source = "useMemo(() => x, [Object.assign({}, base)]);";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_spread_of_object_literal() {
        let source = "useMemo(() => x, [{ ...{ a: 1 } }]);";
        // The spread is inside an object, so the object literal itself is
        // what the dep array holds; still one violation.
        assert_eq!(run_on(source).len(), 1);
    }

    // --- Negative cases: must NOT flag ---

    #[test]
    fn allows_bare_identifier() {
        let source = "useCallback(fn, [foo]);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_member_access() {
        let source = "useCallback(fn, [foo.bar]);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_member_access_chain() {
        let source = "useEffect(() => {}, [user.profile.id]);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_multiple_identifiers() {
        let source = "useCallback(fn, [a, b, c]);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_index_access() {
        let source = "useCallback(fn, [items[0]]);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_ternary_of_identifiers() {
        let source = "useCallback(fn, [cond ? a : b]);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_binary_expression() {
        let source = "useCallback(fn, [a + b]);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_template_literal() {
        let source = "useCallback(fn, [`${a}-${b}`]);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_primitive_keywords() {
        let source = "useEffect(() => {}, [undefined, null, true]);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_empty_dep_array() {
        let source = "useEffect(() => { init(); }, []);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_no_dep_array() {
        let source = "useEffect(() => { init(); });";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_arbitrary_call_expression() {
        // `getFoo()` could return a stable ref; we don't guess.
        let source = "useCallback(fn, [getFoo()]);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_user_defined_new_expression() {
        // Unknown user constructor — don't guess.
        let source = "useMemo(() => x, [new MyThing()]);";
        assert!(run_on(source).is_empty());
    }
}
