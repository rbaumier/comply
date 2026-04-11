//! prefer-prototype-methods backend — borrow from prototypes, not literal instances.

use crate::diagnostic::{Diagnostic, Severity};

/// (literal_prefix, method, call_method, constructor, replacement)
const OBJECT_PATTERNS: &[(&str, &str, &str)] = &[
    ("hasOwnProperty", "Object", "Object.prototype.hasOwnProperty"),
    ("isPrototypeOf", "Object", "Object.prototype.isPrototypeOf"),
    ("propertyIsEnumerable", "Object", "Object.prototype.propertyIsEnumerable"),
    ("toLocaleString", "Object", "Object.prototype.toLocaleString"),
    ("toString", "Object", "Object.prototype.toString"),
    ("valueOf", "Object", "Object.prototype.valueOf"),
];

const ARRAY_PATTERNS: &[(&str, &str, &str)] = &[
    ("slice", "Array", "Array.prototype.slice"),
    ("map", "Array", "Array.prototype.map"),
    ("forEach", "Array", "Array.prototype.forEach"),
    ("filter", "Array", "Array.prototype.filter"),
    ("concat", "Array", "Array.prototype.concat"),
    ("indexOf", "Array", "Array.prototype.indexOf"),
    ("join", "Array", "Array.prototype.join"),
    ("push", "Array", "Array.prototype.push"),
    ("splice", "Array", "Array.prototype.splice"),
    ("reduce", "Array", "Array.prototype.reduce"),
    ("find", "Array", "Array.prototype.find"),
    ("includes", "Array", "Array.prototype.includes"),
    ("some", "Array", "Array.prototype.some"),
    ("every", "Array", "Array.prototype.every"),
    ("flat", "Array", "Array.prototype.flat"),
    ("flatMap", "Array", "Array.prototype.flatMap"),
];

/// Delegation methods: `.call(`, `.apply(`, `.bind(`
const DELEGATION: &[&str] = &["call", "apply", "bind"];

crate::ast_check! { |node, source, ctx, diagnostics|
    // We look for: `{}.method.call(…)` or `[].method.call(…)`.
    // In tree-sitter this is:
    //   call_expression
    //     function: member_expression
    //       object: member_expression         ← `{}.method` or `[].method`
    //         object: object / array           ← `{}` or `[]`
    //         property: identifier             ← method name
    //       property: identifier               ← `call`/`apply`/`bind`
    if node.kind() != "call_expression" {
        return;
    }

    let Some(func) = node.child_by_field_name("function") else { return };
    if func.kind() != "member_expression" { return; }

    // The outer property must be call/apply/bind.
    let Some(outer_prop) = func.child_by_field_name("property") else { return };
    let delegation = outer_prop.utf8_text(source).unwrap_or("");
    if !DELEGATION.contains(&delegation) { return; }

    // The object of the outer member_expression must also be a member_expression.
    let Some(inner) = func.child_by_field_name("object") else { return };
    if inner.kind() != "member_expression" { return; }

    let Some(literal) = inner.child_by_field_name("object") else { return };
    let Some(method_node) = inner.child_by_field_name("property") else { return };
    let method = method_node.utf8_text(source).unwrap_or("");

    // Unwrap parenthesized_expression: `({})` -> `{}`
    let literal = if literal.kind() == "parenthesized_expression" {
        let Some(inner) = literal.named_child(0) else { return };
        inner
    } else {
        literal
    };

    // Check if `literal` is `{}` (empty object) or `[]` (empty array).
    let (is_object, is_array) = match literal.kind() {
        "object" => (literal.named_child_count() == 0, false),
        "array" => (false, literal.named_child_count() == 0),
        _ => return,
    };

    let patterns: &[(&str, &str, &str)] = if is_object {
        OBJECT_PATTERNS
    } else if is_array {
        ARRAY_PATTERNS
    } else {
        return;
    };

    let Some((_method, constructor, replacement)) = patterns.iter().find(|(m, _, _)| *m == method) else { return };

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "prefer-prototype-methods".into(),
        message: format!(
            "Prefer `{replacement}.{delegation}(…)` over borrowing from a literal instance. \
             Use `{constructor}.prototype.{method}` instead."
        ),
        severity: Severity::Warning,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_object_has_own_property_call() {
        let d = run_on("const has = ({}).hasOwnProperty.call(obj, 'key');");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Object.prototype.hasOwnProperty"));
    }

    #[test]
    fn flags_object_to_string_call() {
        let d = run_on("const type = ({}).toString.call(value);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Object.prototype.toString"));
    }

    #[test]
    fn flags_array_slice_call() {
        let d = run_on("const args = [].slice.call(arguments);");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Array.prototype.slice"));
    }

    #[test]
    fn flags_array_map_call() {
        let d = run_on("[].map.call(nodeList, fn)");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("Array.prototype.map"));
    }

    #[test]
    fn allows_prototype_methods() {
        assert!(run_on("Object.prototype.hasOwnProperty.call(obj, 'key')").is_empty());
    }

    #[test]
    fn allows_normal_method_calls() {
        assert!(run_on("obj.hasOwnProperty('key')").is_empty());
    }
}
