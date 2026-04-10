//! react-no-object-in-dep-array backend for TypeScript / TSX.
//!
//! Detects `useEffect`, `useMemo`, `useCallback` calls whose last
//! argument is an array literal containing a bare identifier (not a
//! member expression like `user.id`). A bare identifier in a dep array
//! almost always means an object or array reference that changes every
//! render, causing infinite loops or wasted work.
//!
//! We deliberately DON'T flag member expressions (`user.id`, `items.length`)
//! because those are exactly the fix the rule recommends.

use crate::diagnostic::{Diagnostic, Severity};

const HOOKS: &[&str] = &["useEffect", "useMemo", "useCallback"];

crate::ast_check! { |node, source, ctx, diagnostics|
    let Some(fn_name) = crate::rules::call_expression::call_function_name(node, source) else {
        return;
    };
    if !HOOKS.contains(&fn_name) {
        return;
    }
    // The dep array is the last argument. In tree-sitter-typescript it's
    // an `arguments` node whose last named child is an `array`.
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
    // Walk each element of the dep array. Flag bare identifiers (not
    // member_expression, not string, not number).
    let mut dep_cursor = last_arg.walk();
    for dep in last_arg.named_children(&mut dep_cursor) {
        if dep.kind() != "identifier" {
            continue; // member_expression, literal, etc. — fine.
        }
        let Ok(dep_name) = dep.utf8_text(source) else {
            continue;
        };
        // Common primitives that are safe as bare identifiers.
        if matches!(dep_name, "undefined" | "null" | "true" | "false") {
            continue;
        }
        let pos = dep.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "react-no-object-in-dep-array".into(),
            message: format!(
                "`{dep_name}` in `{fn_name}` dep array — if this is an object \
                 or array, it changes reference every render and causes infinite \
                 re-runs. Extract the primitive field: `{dep_name}.id`, \
                 `{dep_name}.length`, etc."
            ),
            severity: Severity::Error,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_bare_identifier_in_useeffect_deps() {
        let source = "useEffect(() => { console.log(user); }, [user]);";
        assert_eq!(run_on(source).len(), 1);
        assert!(run_on(source)[0].message.contains("user"));
    }

    #[test]
    fn allows_member_expression_in_deps() {
        let source = "useEffect(() => { console.log(user.id); }, [user.id]);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_empty_dep_array() {
        let source = "useEffect(() => { init(); }, []);";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_usememo_with_object_dep() {
        let source = "const val = useMemo(() => calc(data), [data]);";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_no_dep_array() {
        let source = "useEffect(() => { init(); });";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_primitive_keywords() {
        let source = "useEffect(() => {}, [undefined, null, true]);";
        assert!(run_on(source).is_empty());
    }
}
