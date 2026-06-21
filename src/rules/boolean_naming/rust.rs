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
    fn does_not_flag_word_starting_with_prefix_letters() {
        // `issuer` starts with `is` letters but is not a boolean predicate.
        // It won't be flagged because its type isn't bool.
        assert!(run_on("fn f() { let issuer: &str = \"ACME\"; }").is_empty());
    }
}
