//! bool-param-default backend — flag optional boolean parameters with no default.
//!
//! Detection:
//! - Walk `optional_parameter` nodes (e.g. `secure?: boolean`) whose type
//!   annotation resolves to `boolean`.
//! - Also walk `required_parameter` nodes whose type is a union containing
//!   `undefined` (e.g. `secure: boolean | undefined`) and that have no
//!   default value.
//! - Exclude parameters inside interface/type-level signatures
//!   (`function_signature`, `method_signature`, `call_signature`,
//!   `construct_signature`, `abstract_method_signature`): there the shape is
//!   imposed by the declaring type and adding a default is not possible.

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    let kind = node.kind();
    if kind != "optional_parameter" && kind != "required_parameter" {
        return;
    }

    // Must sit directly inside a `formal_parameters` list (skip destructuring
    // patterns and other reuses of the same node kind).
    let Some(parent) = node.parent() else { return; };
    if parent.kind() != "formal_parameters" {
        return;
    }

    // Skip type-level signatures where the parameter shape is imposed.
    if enclosing_is_signature(parent) {
        return;
    }

    // Having a default value means the author already made the behavior
    // explicit — nothing to flag.
    if has_default_value(node) {
        return;
    }

    let Some(type_node) = find_type_annotation(node) else { return; };

    let optional = kind == "optional_parameter";
    if !type_describes_optional_boolean(type_node, source, optional) {
        return;
    }

    let name = param_name(node, source).unwrap_or("<param>");
    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: ctx.path.to_path_buf(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "bool-param-default".into(),
        message: format!(
            "Optional boolean parameter '{name}' has no default value — \
             replace `?: boolean` with `: boolean = <default>` so call \
             sites that omit it have an unambiguous behavior."
        ),
        severity: Severity::Warning,
        span: None,
    });
}

/// True if the parameter list belongs to an interface-/type-level signature,
/// where the caller can't add a default.
fn enclosing_is_signature(formal_params: tree_sitter::Node) -> bool {
    let Some(parent) = formal_params.parent() else { return false; };
    matches!(
        parent.kind(),
        "function_signature"
            | "method_signature"
            | "call_signature"
            | "construct_signature"
            | "abstract_method_signature"
            | "function_type"
            | "constructor_type"
    )
}

/// `required_parameter` with `value` field means `x = default`.
/// For `optional_parameter` the presence of `value` means `x?: T = default`
/// — rare but legal; TS then treats the param as optional-with-default.
fn has_default_value(node: tree_sitter::Node) -> bool {
    node.child_by_field_name("value").is_some()
}

fn find_type_annotation(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_annotation" {
            // The inner type sits after the `:` token — return the first
            // non-token child if there is one, else the annotation itself.
            let mut ta_cursor = child.walk();
            if let Some(gc) = child.named_children(&mut ta_cursor).next() {
                return Some(gc);
            }
            return Some(child);
        }
    }
    None
}

/// For an `optional_parameter`, any `boolean` type qualifies.
/// For a `required_parameter`, the type must be a union including `undefined`
/// (e.g. `boolean | undefined`) to be considered optional in the SonarJS sense.
fn type_describes_optional_boolean(
    type_node: tree_sitter::Node,
    source: &[u8],
    optional: bool,
) -> bool {
    if optional {
        return is_boolean_type(type_node, source);
    }
    if type_node.kind() != "union_type" {
        return false;
    }
    let mut has_boolean = false;
    let mut has_undefined = false;
    let mut cursor = type_node.walk();
    for child in type_node.named_children(&mut cursor) {
        if is_boolean_type(child, source) {
            has_boolean = true;
        } else if is_undefined_type(child, source) {
            has_undefined = true;
        }
    }
    has_boolean && has_undefined
}

fn is_boolean_type(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.kind() == "predefined_type"
        && node.utf8_text(source).is_ok_and(|t| t.trim() == "boolean")
}

fn is_undefined_type(node: tree_sitter::Node, source: &[u8]) -> bool {
    match node.kind() {
        "predefined_type" => node.utf8_text(source).is_ok_and(|t| t.trim() == "undefined"),
        "literal_type" => node.utf8_text(source).is_ok_and(|t| t.trim() == "undefined"),
        _ => false,
    }
}

fn param_name<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(source).ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_optional_boolean_no_default() {
        let diags = run_on("function connect(host: string, secure?: boolean) {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'secure'"));
    }

    #[test]
    fn flags_multiple_optional_boolean_params() {
        let diags = run_on("function f(a?: boolean, b?: boolean) {}");
        assert_eq!(diags.len(), 2);
    }

    #[test]
    fn flags_union_with_undefined_no_default() {
        let diags = run_on("function f(cache: boolean | undefined) {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'cache'"));
    }

    #[test]
    fn flags_optional_boolean_in_arrow() {
        let diags = run_on("const f = (secure?: boolean) => secure;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_optional_boolean_in_method() {
        let diags = run_on("class C { m(flag?: boolean) {} }");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_optional_boolean_with_default() {
        // `secure: boolean = true` — tree-sitter treats this as a required_parameter
        // with a `value` field, so the rule naturally does not fire.
        assert!(run_on("function connect(host: string, secure: boolean = true) {}").is_empty());
    }

    #[test]
    fn allows_inferred_default_shorthand() {
        assert!(run_on("function f(cache = false) {}").is_empty());
    }

    #[test]
    fn allows_optional_non_boolean_param() {
        assert!(run_on("function f(count?: number) {}").is_empty());
    }

    #[test]
    fn allows_required_boolean_param() {
        assert!(run_on("function f(flag: boolean) {}").is_empty());
    }

    #[test]
    fn allows_optional_boolean_in_interface_method() {
        // Interface signatures cannot carry defaults — caller can't fix this.
        assert!(run_on("interface I { connect(host: string, secure?: boolean): void; }").is_empty());
    }

    #[test]
    fn allows_optional_boolean_in_callback_type() {
        // A callback type inside a `type` alias — signature-level, no fix possible.
        assert!(run_on("type Cb = (secure?: boolean) => void;").is_empty());
    }

    #[test]
    fn allows_optional_boolean_in_overload_signature() {
        // Overload signatures have no body and no default capability.
        assert!(run_on(
            "function f(host: string, secure?: boolean): void; \
             function f(host: string, secure: boolean = true): void {}"
        ).is_empty());
    }
}
