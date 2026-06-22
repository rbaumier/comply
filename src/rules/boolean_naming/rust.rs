//! boolean-naming backend for Rust.
//!
//! Why: the skill rule "booleans must start with is/has/should/can/will/did/was"
//! applies to Rust too, using snake_case conventions (`is_ready`, `has_items`).
//! Clippy has no equivalent — this is a comply-specific opinionated check.
//!
//! Detection: walk `let_declaration` and `parameter` nodes whose type is
//! `bool` (via `primitive_type` child) or whose initializer is a
//! `boolean_literal`. Check the identifier against the valid prefix list.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_in_test_context;

// Predicate prefixes accepted by the rule. The first row is the classic
// API-surface set (`is_ready`, `has_items`, `should_retry`, …). The
// second row covers loop/state-machine idioms that read as predicates
// in English: `in_string` ("currently inside a string literal?"),
// `seen_private` ("has this branch been traversed?"), `found_return`
// ("did the scan land on its target?"). Forcing `is_in_string` etc.
// adds syllables without information. The third row covers option/toggle
// verb-modal prefixes idiomatic for boolean config parameters:
// `allow_empty` ("allow empty?"), `use_tls` ("use TLS?"),
// `always_quote` ("always quote?"), `with_header` ("with header?").
// `is_allow_empty` would be grammatically wrong.
const VALID_PREFIXES: &[&str] = &[
    "is_", "has_", "should_", "can_", "will_", "did_", "was_", "had_", "in_", "seen_", "found_",
    "allow_", "use_", "always_", "with_",
];

const IDIOMATIC_NAMES: &[&str] = &[
    "done", "success", "ok", "valid", "ready", "closed", "connected",
    "available", "empty", "alive", "enabled", "active", "matched",
    "called", "polled", "changed", "updated", "exists", "loaded",
    "running", "finished", "completed", "started", "stopped",
    "pending", "stall", "eof",
];
// Boolean field names mandated verbatim by an external platform API, which the
// developer cannot rename. `hour12` is the ECMA-402 `Intl.DateTimeFormat`
// option key; a faithful Rust port of the spec must mirror it exactly.
const API_MANDATED_NAMES: &[&str] = &["hour12"];

const NEGATIVE_SUBSTRINGS: &[&str] = &["_not_", "isnt_", "cannot_", "shouldnt_"];

// Predicate words that, when appearing as a separated mid-name word
// (`<noun>_is_<adjective>`, `<noun>_has_<noun>`, …), embed a predicate
// relationship just as a leading prefix does. `sign_is_mandatory`,
// `year_is_six_digits`, `date_is_present` read as "the sign is mandatory" —
// the `_is_` serves the exact semantic function of an `is_` prefix, so a
// redundant leading `is_` would be wrong. Each entry is matched bounded by
// underscores on both sides, so a substring like `axis_` (no leading `_is`)
// or `enabled` is unaffected and still requires a real prefix.
const INFIX_PREDICATES: &[&str] = &["_is_", "_has_", "_should_", "_can_", "_will_"];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["let_declaration", "parameter"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if let Some(d) = check_node(node, ctx.source.as_bytes(), ctx.path) {
            diagnostics.push(d);
        }
    }
}

fn check_node(
    node: tree_sitter::Node,
    source: &[u8],
    path: &std::path::Path,
) -> Option<Diagnostic> {
    if node.kind() != "let_declaration" && node.kind() != "parameter" {
        return None;
    }
    if !has_boolean_type_or_value(node, source) {
        return None;
    }
    let name = extract_identifier(node, source)?;
    if is_std_net_toggle_setter_param(node, name, source) {
        return None;
    }
    if is_assertion_value_param(node, name, source) {
        return None;
    }
    if is_wasm_bindgen_foreign_param(node, source) {
        return None;
    }
    if is_loop_iteration_toggle(node, name, source) {
        return None;
    }
    if is_builder_setter_field_param(node, name, source) {
        return None;
    }
    let problem = classify_name(name)?;
    let pos = node.start_position();
    Some(Diagnostic {
        path: path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "boolean-naming".into(),
        message: format!(
            "Boolean '{name}' {problem}. Use a predicate prefix: \
             `is_*`, `has_*`, `should_*`, `can_*`, `will_*`, `did_*`, `was_*`, \
             `in_*`, `seen_*`, `found_*`, `allow_*`, `use_*`, `always_*`, `with_*`."
        ),
        severity: Severity::Warning,
        span: None,
    })
}

/// True if the let_declaration / parameter has `: bool` annotation or is
/// initialized with a boolean literal.
fn has_boolean_type_or_value(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "primitive_type" => {
                if child.utf8_text(source).is_ok_and(|t| t == "bool") {
                    return true;
                }
            }
            "boolean_literal" => return true,
            _ => {}
        }
    }
    false
}

/// True for a `bool` parameter named exactly `on`/`off` on a `set_*` method
/// with a `self` receiver — the std::net toggle-setter convention
/// (`UdpSocket::set_broadcast(&self, on: bool)`, `set_multicast_loop_v4`, …).
/// async/wrapping crates mirror this signature verbatim, so forcing `is_on`
/// would make the wrapper diverge from the API it reproduces.
///
/// Anchored on three AST signals so it cannot widen into a name allowlist:
/// the node is a `parameter` whose name is `on`/`off`, its directly-enclosing
/// `function_item` `name` field starts with `set_`, and that function's
/// `parameters` declare a `self_parameter` receiver. A `bool` param named `on`
/// in a free function, a non-`set_*` method, or any other unprefixed boolean
/// is unaffected and still flags. The walk stops at the first
/// `closure_expression` boundary so a closure callback param named `on`/`off`
/// nested inside a `set_*` method is not exempted.
fn is_std_net_toggle_setter_param(
    node: tree_sitter::Node,
    name: &str,
    source: &[u8],
) -> bool {
    if node.kind() != "parameter" || (name != "on" && name != "off") {
        return false;
    }
    let mut cursor = node;
    while let Some(parent) = cursor.parent() {
        if parent.kind() == "closure_expression" {
            return false;
        }
        if parent.kind() == "function_item" {
            let starts_with_set = parent
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .is_some_and(|fn_name| fn_name.starts_with("set_"));
            return starts_with_set && method_has_self_receiver(parent);
        }
        cursor = parent;
    }
    false
}

/// True if `function_item`'s `parameters` declare a `self_parameter` receiver.
fn method_has_self_receiver(function_item: tree_sitter::Node) -> bool {
    let Some(params) = function_item.child_by_field_name("parameters") else {
        return false;
    };
    let mut cursor = params.walk();
    params
        .children(&mut cursor)
        .any(|child| child.kind() == "self_parameter")
}

/// True for the consuming-builder field-setter convention: a `bool` parameter
/// whose name is identical to the enclosing method's name, where the method
/// takes `self` by value and returns `Self`
/// (`pub fn fit_intercept(mut self, fit_intercept: bool) -> Self`). Here the
/// parameter name is dictated by the field it sets, named after that field per
/// the builder convention, not chosen freely — so a predicate prefix would
/// diverge the parameter from the field and method it mirrors.
///
/// Anchored on three AST signals so it cannot widen into a name allowlist: the
/// node is a `parameter` whose name equals the enclosing `function_item`'s
/// `name` field, that function takes a by-value `self` receiver, and its return
/// type is `Self`. A free function, a `&self`/`&mut self` accessor, a setter not
/// returning `Self`, or any parameter whose name differs from the method name is
/// unaffected and still flags. The walk stops at the first `closure_expression`
/// boundary so a closure callback param is judged by its own scope.
fn is_builder_setter_field_param(
    node: tree_sitter::Node,
    name: &str,
    source: &[u8],
) -> bool {
    if node.kind() != "parameter" {
        return false;
    }
    let mut cursor = node;
    while let Some(parent) = cursor.parent() {
        if parent.kind() == "closure_expression" {
            return false;
        }
        if parent.kind() == "function_item" {
            let name_matches_method = parent
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .is_some_and(|fn_name| fn_name == name);
            return name_matches_method
                && method_has_by_value_self_receiver(parent, source)
                && method_returns_self(parent, source);
        }
        cursor = parent;
    }
    false
}

/// True if `function_item`'s `parameters` declare a by-value `self` receiver
/// (`self` / `mut self`), as opposed to `&self` / `&mut self`. A consuming
/// builder setter takes `self` by value; the borrowed forms are accessors.
fn method_has_by_value_self_receiver(function_item: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(params) = function_item.child_by_field_name("parameters") else {
        return false;
    };
    let mut cursor = params.walk();
    params.children(&mut cursor).any(|child| {
        // `&self` / `&mut self` / `&'a self` all contain `&`; by-value forms
        // (`self` / `mut self`) do not.
        child.kind() == "self_parameter"
            && !child.utf8_text(source).is_ok_and(|t| t.contains('&'))
    })
}

/// True if `function_item`'s declared return type is `Self`.
fn method_returns_self(function_item: tree_sitter::Node, source: &[u8]) -> bool {
    function_item
        .child_by_field_name("return_type")
        .and_then(|n| n.utf8_text(source).ok())
        .is_some_and(|t| t == "Self")
}

/// True for a `bool` parameter named exactly `expected`/`actual` on a
/// test/assertion helper. `assert_eq!(expected, actual)` is the universal
/// convention for naming the asserted value, so `expected: bool` reads as
/// "the value the test expects", not as a state predicate; forcing `is_expected`
/// would misname it (it names the assertion's expected value, not a predicate on
/// some noun). The rule already accepts `expected: i32`/`&str`; this aligns the
/// `bool` case.
///
/// Anchored on the param name AND a structural test/assertion context, so it
/// cannot widen into a name allowlist. The node must be a `parameter` named
/// exactly `expected`/`actual`, and the enclosing `function_item` must be a
/// test/assertion helper — established by ANY of:
/// - `is_in_test_context` (a `#[cfg(test)]` module or test-attribute ancestor,
///   covering helpers inside a `#[cfg(test)] mod` in a normal `src` file);
/// - the enclosing `function_item` name begins with `assert`/`expect`/`check`/
///   `test` (assertion-helper naming); or
/// - the enclosing `function_item` body contains an assertion macro invocation
///   (`assert*!`/`debug_assert*!`), which is the issue's shape: a helper named
///   `case` whose body is `assert_eq!(expected, …)`.
///
/// A production `expected: bool` parameter with no test/assertion context is
/// unaffected and still flags. The walk stops at the first `closure_expression`
/// boundary so a closure callback param named `expected`/`actual` is judged by
/// its own enclosing function.
fn is_assertion_value_param(node: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    if node.kind() != "parameter" || (name != "expected" && name != "actual") {
        return false;
    }
    if is_in_test_context(node, source) {
        return true;
    }
    let mut cursor = node;
    while let Some(parent) = cursor.parent() {
        if parent.kind() == "closure_expression" {
            return false;
        }
        if parent.kind() == "function_item" {
            return fn_name_is_assertion_helper(parent, source)
                || fn_body_contains_assertion(parent, source);
        }
        cursor = parent;
    }
    false
}

/// True if `function_item`'s `name` begins with an assertion-helper verb
/// (`assert`/`expect`/`check`/`test`).
fn fn_name_is_assertion_helper(function_item: tree_sitter::Node, source: &[u8]) -> bool {
    function_item
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .is_some_and(|fn_name| {
            ["assert", "expect", "check", "test"]
                .iter()
                .any(|prefix| fn_name.starts_with(prefix))
        })
}

/// True if `function_item`'s body contains an assertion macro invocation
/// (`assert!`/`assert_eq!`/`assert_ne!`/`debug_assert*!`).
fn fn_body_contains_assertion(function_item: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(body) = function_item.child_by_field_name("body") else {
        return false;
    };
    let mut cursor = body.walk();
    let mut stack = vec![body];
    while let Some(current) = stack.pop() {
        if current.kind() == "macro_invocation"
            && current
                .child_by_field_name("macro")
                .and_then(|m| m.utf8_text(source).ok())
                .is_some_and(is_assertion_macro_name)
        {
            return true;
        }
        stack.extend(current.children(&mut cursor));
    }
    false
}

/// True if `name` is an assertion macro: `assert`, `assert_eq`, `assert_ne`,
/// or any `debug_assert*` counterpart.
fn is_assertion_macro_name(name: &str) -> bool {
    matches!(name, "assert" | "assert_eq" | "assert_ne")
        || matches!(name, "debug_assert" | "debug_assert_eq" | "debug_assert_ne")
}

/// Proc macros that rewrite an `extern` block into safe foreign-binding
/// interop whose function signatures mirror an external API verbatim. A
/// parameter in such a block carries the name dictated by the bound API (e.g.
/// the Web IDL attribute names `cancelable`, `bubbles` in wasm-bindgen's
/// `web-sys` bindings) and cannot be renamed by the developer.
const BINDING_MACRO_ATTRS: &[&str] = &["wasm_bindgen"];

/// True for a `bool` parameter declared inside a `foreign_mod_item`
/// (`extern "C" { … }`) annotated with a binding-generation proc macro
/// (`#[wasm_bindgen]`). wasm-bindgen's `web-sys` bindings declare DOM/Web API
/// methods whose parameter names are the exact Web IDL attribute names
/// (`cancelable`, `bubbles`, `ctrl_key`, `alt_key`, …); the signature is
/// dictated by the bound JavaScript API, so forcing an `is_` prefix would
/// diverge from the spec and break the 1:1 mapping developers rely on.
///
/// Anchored on two AST signals so it cannot widen into a name allowlist: the
/// node is a `parameter`, and walking up its ancestors (stopping at the first
/// `closure_expression` boundary) reaches a `foreign_mod_item` whose preceding
/// outer attributes include a binding-generation macro. An unprefixed boolean
/// in an ordinary function — or in an `extern` block without such an attribute
/// — is unaffected and still requires a predicate prefix.
fn is_wasm_bindgen_foreign_param(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "parameter" {
        return false;
    }
    let mut cursor = node;
    while let Some(parent) = cursor.parent() {
        if parent.kind() == "closure_expression" {
            return false;
        }
        if parent.kind() == "foreign_mod_item" {
            return has_binding_macro_attr(parent, source);
        }
        cursor = parent;
    }
    false
}

/// True if any outer attribute immediately preceding the `foreign_mod_item`
/// names a binding-generation proc macro (see [`BINDING_MACRO_ATTRS`]). Outer
/// attributes are preceding siblings of the block, optionally separated from it
/// by comments, so the scan walks back over `attribute_item` siblings and skips
/// interleaved comments.
fn has_binding_macro_attr(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = node.prev_sibling();
    while let Some(prev) = sibling {
        match prev.kind() {
            "line_comment" | "block_comment" => {}
            "attribute_item" => {
                if attr_path_head(prev, source).is_some_and(|head| {
                    BINDING_MACRO_ATTRS.contains(&head)
                }) {
                    return true;
                }
            }
            _ => break,
        }
        sibling = prev.prev_sibling();
    }
    false
}

/// The leading path identifier of an `attribute_item`, e.g. `wasm_bindgen` for
/// both `#[wasm_bindgen]` and `#[wasm_bindgen(method)]`. Returns `None` when the
/// attribute's path is not a bare identifier (a scoped path like `crate::foo`
/// never names a binding macro here).
fn attr_path_head<'a>(attribute_item: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut item_cursor = attribute_item.walk();
    let attribute = attribute_item
        .children(&mut item_cursor)
        .find(|child| child.kind() == "attribute")?;
    let path = attribute.named_child(0)?;
    if path.kind() != "identifier" {
        return None;
    }
    path.utf8_text(source).ok()
}

/// Names that read as a first-iteration sentinel in the separator/join idiom.
const ITERATION_TOGGLE_NAMES: &[&str] = &["first"];

/// True for the canonical separator/join idiom: a `let` binding named `first`
/// initialized to a boolean literal (`let mut first = true;`) and reassigned to
/// a boolean literal (`first = false;`) inside an enclosing loop body. Such a
/// binding tracks whether the current iteration is the first one, so its value
/// changes across iterations — an iteration flag, not an ordinary boolean.
///
/// Anchored on the name AND the init-literal AND an in-loop reassignment, so it
/// cannot widen into a name allowlist. The node must be a `let_declaration`
/// named exactly `first`, initialized with a boolean literal, and there must be
/// a `for`/`while`/`loop` within the enclosing function body that reassigns
/// `first` to a boolean literal. A `first: bool` parameter, a `first` binding
/// with no in-loop reassignment, or a `first` reassigned only outside a loop is
/// an ordinary boolean and still requires a predicate prefix.
fn is_loop_iteration_toggle(node: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    if node.kind() != "let_declaration" || !ITERATION_TOGGLE_NAMES.contains(&name) {
        return false;
    }
    if !initialized_with_boolean_literal(node, source) {
        return false;
    }
    let Some(scope) = enclosing_function_body(node) else {
        return false;
    };
    loop_body_reassigns_to_bool_literal(scope, name, source)
}

/// True if a `let_declaration` has a `= true` / `= false` initializer.
fn initialized_with_boolean_literal(node: tree_sitter::Node, source: &[u8]) -> bool {
    node.child_by_field_name("value")
        .is_some_and(|value| value.kind() == "boolean_literal" && value.utf8_text(source).is_ok())
}

/// Walk up to the nearest enclosing function/closure body (`block`), which
/// bounds the search for the in-loop reassignment.
fn enclosing_function_body(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cursor = node.parent();
    let mut last_block = None;
    while let Some(parent) = cursor {
        if parent.kind() == "block" {
            last_block = Some(parent);
        }
        if parent.kind() == "function_item" || parent.kind() == "closure_expression" {
            break;
        }
        cursor = parent.parent();
    }
    last_block
}

/// True if any loop (`for`/`while`/`loop`) within `scope` reassigns `name` to a
/// boolean literal — `name = true` / `name = false`.
fn loop_body_reassigns_to_bool_literal(
    scope: tree_sitter::Node,
    name: &str,
    source: &[u8],
) -> bool {
    let mut cursor = scope.walk();
    let mut stack = vec![scope];
    while let Some(current) = stack.pop() {
        if matches!(
            current.kind(),
            "for_expression" | "while_expression" | "loop_expression"
        ) && subtree_reassigns_to_bool_literal(current, name, source)
        {
            return true;
        }
        stack.extend(current.children(&mut cursor));
    }
    false
}

/// True if `node`'s subtree contains `name = <boolean_literal>`.
fn subtree_reassigns_to_bool_literal(
    node: tree_sitter::Node,
    name: &str,
    source: &[u8],
) -> bool {
    let mut cursor = node.walk();
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        if current.kind() == "assignment_expression"
            && current
                .child_by_field_name("left")
                .is_some_and(|left| left.kind() == "identifier"
                    && left.utf8_text(source).is_ok_and(|t| t == name))
            && current
                .child_by_field_name("right")
                .is_some_and(|right| right.kind() == "boolean_literal")
        {
            return true;
        }
        stack.extend(current.children(&mut cursor));
    }
    false
}

fn extract_identifier<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(source).ok();
        }
    }
    None
}

/// True if the name ends in the explicit `flag` suffix as a distinct word
/// (`use_delta_flag` or bare `flag`). The `flag` suffix is itself a boolean
/// marker — as clear an intent signal as an `is_*`/`has_*` prefix — and is the
/// verbatim naming convention for boolean syntax elements in ITU-T/ISO codec
/// and protocol specifications. A trailing `flag` mid-word (`flagged`) does
/// not match: the snake_case word boundary (`_flag`) is required, so
/// adjective/state names still need a prefix.
fn has_flag_suffix(name: &str) -> bool {
    name == "flag" || name.ends_with("_flag")
}

/// True if `name` embeds a predicate word as a separated mid-name word
/// (`<noun>_is_<adjective>`, `<noun>_has_<noun>`, …). The predicate word is
/// matched bounded by underscores on both sides, and there must be a non-empty
/// noun before it and a non-empty descriptor after it, so the name reads as
/// "the noun is/has X" — the infix `_is_` carries the same intent signal as a
/// leading `is_` prefix. A trailing predicate (`mandatory_is`) or a substring
/// without word boundaries (`axis_value`) does not match.
fn has_infix_predicate(name: &str) -> bool {
    INFIX_PREDICATES.iter().any(|infix| match name.find(infix) {
        Some(pos) => pos > 0 && pos + infix.len() < name.len(),
        None => false,
    })
}

/// Return a short problem description if the name violates the rule.
fn classify_name(name: &str) -> Option<&'static str> {
    if NEGATIVE_SUBSTRINGS.iter().any(|neg| name.contains(neg)) {
        return Some("is negatively phrased — use the positive form with `!`");
    }
    if has_flag_suffix(name) {
        return None;
    }
    for &prefix in VALID_PREFIXES {
        if name.starts_with(prefix) {
            return None;
        }
    }
    if has_infix_predicate(name) {
        return None;
    }
    if IDIOMATIC_NAMES.contains(&name) {
        return None;
    }
    if API_MANDATED_NAMES.contains(&name) {
        return None;
    }
    Some("is missing a predicate prefix")
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn allows_is_prefix() {
        assert!(run_on("fn f() { let is_ready: bool = true; }").is_empty());
    }

    #[test]
    fn allows_has_prefix() {
        assert!(run_on("fn f() { let has_items = true; }").is_empty());
    }

    #[test]
    fn flags_missing_prefix_with_annotation() {
        let diags = run_on("fn f() { let retry: bool = true; }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'retry'"));
    }

    #[test]
    fn flags_inferred_boolean() {
        assert_eq!(run_on("fn f() { let retry = true; }").len(), 1);
    }

    #[test]
    fn flags_param_without_prefix() {
        let diags = run_on("fn f(retry: bool) {}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn does_not_flag_non_boolean() {
        assert!(run_on("fn f() { let name: String = String::new(); }").is_empty());
    }

    #[test]
    fn allows_should_will_did_was() {
        for name in ["should_retry", "will_succeed", "did_fire", "was_loaded"] {
            let source = format!("fn f() {{ let {name}: bool = true; }}");
            assert!(run_on(&source).is_empty(), "'{name}' should be allowed");
        }
    }

    #[test]
    fn allows_had_prefix() {
        assert!(run_on("fn f() { let had_error: bool = false; }").is_empty());
    }

    #[test]
    fn allows_semantic_toggle_prefixes_on_params() {
        for name in ["allow_empty", "use_tls", "always_quote", "with_header"] {
            let source = format!("fn f({name}: bool) {{}}");
            assert!(run_on(&source).is_empty(), "'{name}' should be allowed");
        }
    }

    #[test]
    fn still_flags_bare_adjective_param() {
        for name in ["disabled", "optional", "debug"] {
            let source = format!("fn f({name}: bool) {{}}");
            assert_eq!(run_on(&source).len(), 1, "'{name}' should be flagged");
        }
    }

    #[test]
    fn allows_idiomatic_done() {
        assert!(run_on("fn f() { let done: bool = false; }").is_empty());
    }

    #[test]
    fn allows_idiomatic_success() {
        assert!(run_on("fn f() { let success = true; }").is_empty());
    }

    #[test]
    fn allows_idiomatic_polled() {
        assert!(run_on("fn f() { let polled: bool = false; }").is_empty());
    }

    #[test]
    fn allows_api_mandated_hour12() {
        // `hour12` is the ECMA-402 Intl.DateTimeFormat option key; a faithful
        // Rust port cannot rename it. (Closes #4997)
        assert!(run_on("fn with_hour12(hour12: bool) {}").is_empty());
    }

    #[test]
    fn still_flags_user_defined_unprefixed_boolean() {
        // Strictness preserved: user-controlled names still require a prefix.
        assert_eq!(run_on("fn f() { let disabled: bool = true; }").len(), 1);
    }

    #[test]
    fn allows_flag_suffix() {
        // The explicit `flag` suffix is itself a boolean marker — the verbatim
        // naming convention for boolean syntax elements in ITU-T/ISO codec
        // specs (HEVC/H.265, H.264). (Closes #5065)
        assert!(run_on("fn f() { let sps_temporal_id_nesting_flag: bool = true; }").is_empty());
        assert!(run_on("fn f(use_delta_flag: bool) {}").is_empty());
    }

    #[test]
    fn flag_suffix_does_not_soften_adjective_strictness() {
        // The `flag` suffix only validates a trailing-word `flag`; a mid-word
        // `flag` (e.g. `flagged`) is not the boolean-marker suffix.
        assert_eq!(run_on("fn f() { let flagged: bool = true; }").len(), 1);
    }

    #[test]
    fn allows_std_net_toggle_setter_on_param() {
        // std::net convention: `set_*(&self, on: bool)` toggle setters.
        // async/wrapping crates mirror the signature verbatim. (Closes #5356)
        for src in [
            "impl X { pub fn set_broadcast(&self, on: bool) {} }",
            "impl X { pub fn set_multicast_loop_v4(&self, on: bool) {} }",
            "impl X { fn set_nonblocking(&mut self, on: bool) {} }",
            "impl X { fn set_keepalive(&self, off: bool) {} }",
        ] {
            assert!(run_on(src).is_empty(), "`{src}` should be allowed");
        }
    }

    #[test]
    fn still_flags_on_param_in_non_setter_method() {
        // The exemption is anchored to the `set_` prefix; a non-setter method
        // with `on: bool` still requires a predicate prefix.
        let diags = run_on("impl X { fn handle(&self, on: bool) {} }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'on'"));
    }

    #[test]
    fn still_flags_on_param_in_free_function() {
        // No `self` receiver — not the std::net setter shape.
        assert_eq!(run_on("fn set_broadcast(on: bool) {}").len(), 1);
    }

    #[test]
    fn still_flags_on_param_in_set_assoc_fn_without_receiver() {
        // A `set_*` associated fn in an impl but without a `self` receiver is
        // not a toggle setter; its `on` param still requires a prefix.
        assert_eq!(run_on("impl X { fn set_broadcast(on: bool) {} }").len(), 1);
    }

    #[test]
    fn still_flags_closure_on_param_nested_in_setter() {
        // The walk stops at the closure boundary: a closure callback param
        // named `on` inside a `set_*` method is not the setter's own param.
        let diags = run_on("impl X { fn set_cb(&self) { let f = |on: bool| {}; } }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'on'"));
    }

    #[test]
    fn still_flags_unprefixed_boolean_alongside_setter_exemption() {
        // The setter exemption does not weaken the strict rule elsewhere:
        // a bare adjective local still flags.
        assert_eq!(run_on("fn f() { let disabled: bool = true; }").len(), 1);
    }

    #[test]
    fn allows_expected_bool_param_in_assertion_helper() {
        // `assert_eq!(expected, actual)` convention: `expected: bool` names the
        // value the test asserts, not a predicate. The helper is detected by its
        // body containing an assertion macro. (Closes #5405)
        let src = "fn case(expected: bool, value: T) {\n\
                   assert_eq!(expected, value.is_empty());\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_actual_bool_param_in_assertion_helper() {
        let src = "fn case(actual: bool, value: T) {\n\
                   assert_eq!(true, actual);\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_expected_bool_param_in_cfg_test_module() {
        // A helper inside a `#[cfg(test)] mod` in a normal src file: no path
        // signal, the AST `#[cfg(test)]` ancestor establishes the test context.
        let src = "#[cfg(test)]\nmod tests {\n\
                   fn helper(expected: bool) {}\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_expected_bool_param_by_assertion_helper_name() {
        for fn_name in ["assert_state", "expect_value", "check_flag", "test_it"] {
            let src = format!("fn {fn_name}(expected: bool) {{}}");
            assert!(run_on(&src).is_empty(), "`{fn_name}` should be allowed");
        }
    }

    #[test]
    fn still_flags_expected_bool_param_in_production_fn() {
        // No test context, no assertion macro, non-assertion fn name: strictness
        // is preserved — `expected: bool` still requires a predicate prefix.
        let diags = run_on("fn configure(expected: bool) {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'expected'"));
    }

    #[test]
    fn still_flags_disabled_bool_param_alongside_assertion_exemption() {
        // The exemption is anchored to `expected`/`actual`; a different bare
        // adjective param in the same assertion helper still flags.
        let src = "fn case(expected: bool, disabled: bool) {\n\
                   assert_eq!(expected, disabled);\n}";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'disabled'"));
    }

    #[test]
    fn still_flags_closure_expected_param_nested_in_assertion_helper() {
        // The walk stops at the closure boundary: a closure callback param
        // named `expected` inside an assertion helper is judged by its own
        // (closure) scope, not the helper's assertion context.
        let src = "fn case() {\n\
                   assert!(true);\n\
                   let f = |expected: bool| {};\n}";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'expected'"));
    }

    #[test]
    fn allows_noun_is_adjective_infix_predicate() {
        // The `<noun>_is_<adjective>` compound embeds a predicate relationship:
        // the infix `_is_` reads as "the noun is X", serving the same function
        // as a leading `is_` prefix. (Closes #5464)
        for src in [
            "fn fmt_sign(sign_is_mandatory: bool) {}",
            "fn f() { let year_is_six_digits: bool = true; }",
            "fn f() { let date_is_present = false; }",
        ] {
            assert!(run_on(src).is_empty(), "`{src}` should be allowed");
        }
    }

    #[test]
    fn allows_noun_has_can_should_will_infix_predicate() {
        for name in [
            "value_has_owner",
            "user_can_edit",
            "request_should_retry",
            "task_will_run",
        ] {
            let src = format!("fn f({name}: bool) {{}}");
            assert!(run_on(&src).is_empty(), "`{name}` should be allowed");
        }
    }

    #[test]
    fn infix_predicate_does_not_match_unbounded_substring() {
        // `axis` contains the letters `is` but not a separated `_is_` word, so
        // it still requires a real prefix; strictness is preserved.
        assert_eq!(run_on("fn f(axis_locked: bool) {}").len(), 1);
    }

    #[test]
    fn infix_predicate_requires_noun_before_and_descriptor_after() {
        // A trailing predicate (`mandatory_is`) or a leading one is not the
        // `<noun>_is_<adjective>` shape and still flags.
        assert_eq!(run_on("fn f(mandatory_is: bool) {}").len(), 1);
    }

    #[test]
    fn infix_predicate_still_flags_negative_phrasing() {
        // The negative-substring check runs first: `value_is_not_set` embeds a
        // negation and is flagged as negatively phrased, not exempted.
        let diags = run_on("fn f(value_is_not_set: bool) {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("negatively phrased"));
    }

    #[test]
    fn does_not_flag_word_starting_with_prefix_letters() {
        // `issuer` starts with `is` letters but is not a boolean predicate.
        // It won't be flagged because its type isn't bool.
        assert!(run_on("fn f() { let issuer: &str = \"ACME\"; }").is_empty());
    }

    #[test]
    fn allows_webidl_bool_params_in_wasm_bindgen_extern_block() {
        // wasm-bindgen `web-sys` bindings: parameter names are the exact Web IDL
        // attribute names; the signature is dictated by the bound JS API and
        // cannot be renamed. (Closes #5468)
        let src = "#[wasm_bindgen]\nextern \"C\" {\n\
                   #[wasm_bindgen(method, js_name = \"initDragEvent\")]\n\
                   pub fn init_drag_event(this: &DragEvent, type_: &str, can_bubble: bool, cancelable: bool);\n\
                   #[wasm_bindgen(method, setter, js_name = \"ctrlKey\")]\n\
                   pub fn set_ctrl_key(this: &KeyboardEventInit, val: bool);\n\
                   #[wasm_bindgen(method, setter, js_name = \"bubbles\")]\n\
                   pub fn set_bubbles(this: &KeyboardEventInit, bubbles: bool);\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_webidl_bool_param_in_bare_extern_block_with_wasm_bindgen() {
        // A bare `extern { … }` (implicit "C" ABI) is the shape wasm-bindgen emits;
        // the `#[wasm_bindgen]` attribute is the anchor, not the ABI string.
        let src = "#[wasm_bindgen]\nextern {\n\
                   #[wasm_bindgen(method)]\n\
                   pub fn set_alt_key(this: &MouseEventInit, alt_key: bool);\n\
                   }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_bool_param_in_plain_extern_block() {
        // Strictness preserved: a plain `extern "C"` block with no binding macro
        // is ordinary FFI; an unprefixed boolean param still requires a prefix.
        let src = "extern \"C\" {\n    pub fn set_thing(cancelable: bool);\n}";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'cancelable'"));
    }

    #[test]
    fn still_flags_bool_param_in_ordinary_fn_beside_wasm_bindgen_block() {
        // The exemption is anchored to the `foreign_mod_item` ancestor; an
        // ordinary free function elsewhere in the file still flags.
        let src = "#[wasm_bindgen]\nextern \"C\" {\n\
                   #[wasm_bindgen(method)]\n\
                   pub fn set_bubbles(this: &EventInit, bubbles: bool);\n\
                   }\n\
                   fn configure(cancelable: bool) {}";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'cancelable'"));
    }

    #[test]
    fn still_flags_bool_param_in_non_wasm_extern_block_in_module() {
        // A `foreign_mod_item` carrying a non-binding attribute (e.g. `#[link]`)
        // is genuine C FFI, not wasm-bindgen interop; the exemption must not
        // apply and an unprefixed boolean param still flags.
        let src = "#[link(name = \"foo\")]\nextern \"C\" {\n\
                   pub fn set_thing(cancelable: bool);\n}";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'cancelable'"));
    }

    #[test]
    fn allows_first_iteration_toggle_in_loop() {
        // The canonical separator/join idiom: `first` is initialized to a
        // boolean literal and toggled inside a loop body. (Closes #5404)
        let src = "fn f<I: Iterator>(iter: I) {\n\
                   let mut first = true;\n\
                   for token in iter {\n\
                       if !first { op(); }\n\
                       first = false;\n\
                       emit(token);\n\
                   }\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_first_iteration_toggle_in_while_loop() {
        let src = "fn f() {\n\
                   let mut first = true;\n\
                   while next() {\n\
                       if !first { sep(); }\n\
                       first = false;\n\
                   }\n}";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn still_flags_first_with_no_loop_toggle() {
        // A `first` boolean never reassigned inside a loop is an ordinary
        // boolean and still requires a predicate prefix.
        assert_eq!(run_on("fn f() { let first = true; }").len(), 1);
    }

    #[test]
    fn still_flags_first_param() {
        // A `first: bool` parameter is an ordinary boolean, not a loop toggle.
        assert_eq!(run_on("fn f(first: bool) {}").len(), 1);
    }

    #[test]
    fn still_flags_first_reassigned_outside_loop() {
        // Reassignment must be inside a loop body; a plain reassignment outside
        // any loop is not an iteration toggle.
        assert_eq!(run_on("fn f() { let mut first = true; first = false; }").len(), 1);
    }

    #[test]
    fn iteration_toggle_does_not_widen_sibling_booleans() {
        // The exemption is anchored on `first`; a sibling boolean in the same
        // loop scope is unaffected and still flags.
        let src = "fn f<I: Iterator>(iter: I) {\n\
                   let mut first = true;\n\
                   let mut verbose = false;\n\
                   for token in iter {\n\
                       if !first { sep(); }\n\
                       first = false;\n\
                       verbose = true;\n\
                       emit(token);\n\
                   }\n}";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'verbose'"));
    }

    #[test]
    fn allows_builder_setter_field_param() {
        // Consuming-builder field setter: the `bool` param name equals the
        // method name, the method takes `self` by value and returns `Self`.
        // The param is named after the field it sets per builder convention,
        // so a predicate prefix would diverge it from that field. (Closes #5493)
        for src in [
            "impl X { pub fn fit_intercept(mut self, fit_intercept: bool) -> Self { self } }",
            "impl X { pub fn shrinking(mut self, shrinking: bool) -> Self { self } }",
            "impl X { pub fn scale(mut self, scale: bool) -> Self { self } }",
            "impl X { fn symmetric(self, symmetric: bool) -> Self { self } }",
        ] {
            assert!(run_on(src).is_empty(), "`{src}` should be allowed");
        }
    }

    #[test]
    fn still_flags_borrowed_self_accessor_setter() {
        // The exemption is anchored to a by-value `self` receiver; a `&mut self`
        // setter is not a consuming builder and its param still flags.
        let diags = run_on("impl X { fn scale(&mut self, scale: bool) { } }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'scale'"));
    }

    #[test]
    fn still_flags_builder_setter_param_when_name_differs_from_method() {
        // The param name must equal the method name; a differently-named param
        // is not the field-setter shape and still requires a predicate prefix.
        let diags = run_on("impl X { pub fn scale(mut self, disabled: bool) -> Self { self } }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'disabled'"));
    }

    #[test]
    fn still_flags_setter_not_returning_self() {
        // The method must return `Self`; a `self`-consuming method returning a
        // different type is not the builder-setter shape.
        let diags = run_on("impl X { fn scale(mut self, scale: bool) -> X { self } }");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'scale'"));
    }

    #[test]
    fn still_flags_builder_setter_param_in_free_function() {
        // No `self` receiver — a free function named after its param is not a
        // builder setter; the param still flags.
        assert_eq!(run_on("fn scale(scale: bool) -> Self { }").len(), 1);
    }
}
