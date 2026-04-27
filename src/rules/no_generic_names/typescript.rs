//! no-generic-names backend — flags vague/meaningless identifier names
//! along two axes:
//!
//! 1. **Banned words** (exact match): `info`, `temp`, `result`, `obj`,
//!    `item`, `thing`, `stuff`, `val`, `retval`, `value`, `foo`, `bar`.
//!    Only flagged at declaration sites (not every reference).
//!
//! 2. **Banned prefixes** (word-boundary match): `process`, `data`, `do`,
//!    `execute`, `run`, `perform`. These describe mechanics, not intent.
//!    The prefix must be followed by end-of-name, an uppercase letter
//!    (camelCase boundary), or `_` (snake_case boundary) — otherwise
//!    we'd false-positive on `document`, `database`, `domain`,
//!    `performance`, `runtime`.
//!
//! `handle` is intentionally NOT in either list: `handleXxx` is the
//! canonical React event-handler naming convention (`onClick={handleClick}`).
//!
//! Why two modes? A standalone `result = ...` carries no meaning. A
//! compound `dataSource = ...` also carries no meaning — "data" is a
//! filler prefix. Both variants deserve to be flagged from the same
//! rule so users get a single consistent message.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

/// Standalone identifiers with zero semantic content. Flagged only at
/// declaration sites so the rule fires once per variable, not once per
/// use.
const BANNED_WORDS: &[&str] = &[
    "info", "temp", "result", "obj", "item", "thing", "stuff", "val",
    "retval", "value", "foo", "bar",
];

/// Prefixes that describe mechanics rather than intent. Word-boundary
/// matched — `dataSource` matches but `database` does not.
const BANNED_PREFIXES: &[&str] = &["process", "data", "do", "execute", "run", "perform"];

/// Language/runtime globals that happen to start with a banned prefix
/// but must never be flagged — `process.env`, `Buffer.from`,
/// `globalThis.x` are the canonical way to reference these primitives.
/// Exact-name match only; derived names (`processOrder`, `bufferUtil`)
/// still hit the prefix rule.
const GLOBAL_IDENTIFIER_ALLOWLIST: &[&str] = &[
    "process",    // Node global
    "require",    // CJS global
    "module",     // CJS global
    "exports",    // CJS global
    "Buffer",     // Node global
    "globalThis", // JS global
    "console",    // JS global
    "__dirname",  // Node CJS global
    "__filename", // Node CJS global
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["identifier", "property_identifier"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        if let Some(d) = check_banned_word(node, source_bytes, ctx.path) {
            diagnostics.push(d);
            return;
        }
        if let Some(d) = check_banned_prefix(node, source_bytes, ctx.path) {
            diagnostics.push(d);
        }
    }
}

fn check_banned_word(
    node: tree_sitter::Node,
    source: &[u8],
    path: &std::path::Path,
) -> Option<Diagnostic> {
    if node.kind() != "identifier" {
        return None;
    }
    if !is_declaration_site(node) {
        return None;
    }
    if is_destructuring_property(node) {
        return None;
    }
    if is_iterator_callback_param(node, source) {
        return None;
    }
    let name = node.utf8_text(source).ok()?;
    let lower = name.to_ascii_lowercase();
    if !BANNED_WORDS.contains(&lower.as_str()) {
        return None;
    }
    let pos = node.start_position();
    Some(Diagnostic {
        path: path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-generic-names".into(),
        message: format!(
            "Identifier '{name}' carries no meaning — rename to describe \
             what the value IS (`parsedOrder`, `userProfile`, \
             `paymentReceipt`)."
        ),
        severity: Severity::Warning,
        span: None,
    })
}

fn check_banned_prefix(
    node: tree_sitter::Node,
    source: &[u8],
    path: &std::path::Path,
) -> Option<Diagnostic> {
    if node.kind() != "identifier" && node.kind() != "property_identifier" {
        return None;
    }
    if is_destructuring_property(node) {
        return None;
    }
    if is_object_literal_key(node) {
        return None;
    }
    if is_method_call_name(node) {
        return None;
    }
    let name = node.utf8_text(source).ok()?;
    if GLOBAL_IDENTIFIER_ALLOWLIST.contains(&name) {
        return None;
    }
    let prefix = matched_banned_prefix(name)?;
    let pos = node.start_position();
    Some(Diagnostic {
        path: path.to_path_buf().into(),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-generic-names".into(),
        message: format!(
            "Identifier '{name}' uses banned prefix '{prefix}' — use \
             intent over implementation. Try: what does this actually \
             accomplish? (`processOrder` → `fulfillOrder`, `doPayment` → \
             `chargeCustomer`)."
        ),
        severity: Severity::Warning,
        span: None,
    })
}

/// Return the banned prefix matching `name` on a word boundary, or None.
fn matched_banned_prefix(name: &str) -> Option<&'static str> {
    let bytes = name.as_bytes();
    for &prefix in BANNED_PREFIXES {
        let plen = prefix.len();
        if bytes.len() < plen {
            continue;
        }
        if !bytes[..plen].eq_ignore_ascii_case(prefix.as_bytes()) {
            continue;
        }
        let on_boundary = if bytes.len() == plen {
            true
        } else if bytes[..plen].iter().all(|b| b.is_ascii_uppercase()) {
            // SCREAMING_SNAKE_CASE: only `_` is a word boundary.
            // Prevents DATABASE_ERROR matching prefix "data".
            bytes[plen] == b'_'
        } else {
            bytes[plen].is_ascii_uppercase() || bytes[plen] == b'_'
        };
        if on_boundary {
            return Some(prefix);
        }
    }
    None
}

/// True when the identifier appears directly inside a declaring context
/// (variable_declarator, required_parameter, catch_clause) rather than as
/// a reference to an existing binding.
fn is_declaration_site(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    matches!(
        parent.kind(),
        "variable_declarator" | "required_parameter" | "catch_clause"
    )
}

/// True when the name comes from destructuring (`const { data } = ...`) —
/// the user doesn't choose these names, the API imposes them.
fn is_destructuring_property(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() == "shorthand_property_identifier_pattern" {
        return true;
    }
    if parent.kind() == "pair_pattern" {
        if let Some(grandparent) = parent.parent() {
            return grandparent.kind() == "object_pattern";
        }
    }
    if node.kind() == "identifier" {
        if let Some(gp) = parent.parent() {
            if gp.kind() == "object_pattern" {
                return true;
            }
        }
    }
    false
}

/// True when the identifier is a method name being called (`obj.execute()`).
/// API method names are chosen by the library, not the developer.
fn is_method_call_name(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else { return false };
    if parent.kind() != "member_expression" { return false; }
    if parent.child_by_field_name("property").map(|p| p.id()) != Some(node.id()) {
        return false;
    }
    let Some(gp) = parent.parent() else { return false };
    gp.kind() == "call_expression"
}

/// True when the identifier is a property key in an object literal
/// (`{ data: session }` or shorthand `{ data }`). These names are part
/// of a return contract / API shape, not the author's naming choice.
fn is_object_literal_key(node: tree_sitter::Node) -> bool {
    let Some(parent) = node.parent() else { return false };
    // Shorthand property: `{ data }` in an object literal
    if parent.kind() == "shorthand_property_identifier" {
        if let Some(gp) = parent.parent() {
            return gp.kind() == "object";
        }
    }
    // Full property: `{ data: value }` — the key side
    if parent.kind() == "pair" {
        if let Some(key) = parent.child_by_field_name("key") {
            if key.id() == node.id() {
                return true;
            }
        }
    }
    false
}

const ITERATOR_METHODS: &[&str] = &[
    "map", "filter", "find", "findIndex", "forEach", "some", "every",
    "flatMap", "reduce", "sort",
];

/// True when the identifier is a parameter of an inline arrow/function
/// passed directly to an array iterator method (.map(), .filter(), etc.).
fn is_iterator_callback_param(node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(parent) = node.parent() else { return false };
    if parent.kind() != "required_parameter"
        && parent.kind() != "formal_parameters"
        && parent.kind() != "arrow_function"
    {
        return false;
    }
    let func = if parent.kind() == "required_parameter" || parent.kind() == "formal_parameters" {
        parent.parent()
    } else {
        Some(parent)
    };
    let Some(func_node) = func else { return false };
    if func_node.kind() != "arrow_function" && func_node.kind() != "function_expression" {
        if let Some(pp) = func_node.parent() {
            if pp.kind() != "arrow_function" && pp.kind() != "function_expression" {
                return false;
            }
            return is_arg_of_iterator_call(pp, source);
        }
        return false;
    }
    is_arg_of_iterator_call(func_node, source)
}

fn is_arg_of_iterator_call(func_node: tree_sitter::Node, source: &[u8]) -> bool {
    let Some(args) = func_node.parent() else { return false };
    if args.kind() != "arguments" {
        return false;
    }
    let Some(call) = args.parent() else { return false };
    if call.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = call.child_by_field_name("function") else { return false };
    if callee.kind() != "member_expression" {
        return false;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return false };
    let method = prop.utf8_text(source).unwrap_or("");
    ITERATOR_METHODS.contains(&method)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    // --- banned words (exact match at declaration site) ---

    #[test]
    fn flags_const_data_via_prefix() {
        // `data` is now in BANNED_PREFIXES (superset); matches on exact name.
        assert_eq!(run_on("const data = 5;").len(), 1);
    }

    #[test]
    fn flags_let_temp() {
        assert_eq!(run_on("let temp = 1;").len(), 1);
    }

    #[test]
    fn flags_function_param_result() {
        assert_eq!(run_on("function f(result: number) {}").len(), 1);
    }

    #[test]
    fn flags_val() {
        assert_eq!(run_on("const val = 1;").len(), 1);
    }

    #[test]
    fn flags_foo_bar() {
        assert_eq!(run_on("const foo = 1;").len(), 1);
        assert_eq!(run_on("const bar = 1;").len(), 1);
    }

    #[test]
    fn allows_descriptive_names() {
        assert!(run_on("const parsedOrder = 1;").is_empty());
        assert!(run_on("const userProfile = {};").is_empty());
    }

    // --- banned prefixes (word-boundary match on any identifier) ---

    #[test]
    fn flags_process_prefix_camel_case() {
        assert!(run_on("function processOrder() {}")
            .iter()
            .any(|d| d.message.contains("processOrder")));
    }

    #[test]
    fn flags_process_prefix_snake_case() {
        assert!(run_on("const process_order = 1;")
            .iter()
            .any(|d| d.message.contains("process_order")));
    }

    #[test]
    fn flags_do_prefix() {
        assert!(run_on("function doStuff() {}")
            .iter()
            .any(|d| d.message.contains("doStuff")));
    }

    #[test]
    fn flags_execute_prefix() {
        assert!(run_on("function executeSomething() {}")
            .iter()
            .any(|d| d.message.contains("executeSomething")));
    }

    #[test]
    fn flags_run_prefix() {
        assert!(run_on("function runTask() {}")
            .iter()
            .any(|d| d.message.contains("runTask")));
    }

    #[test]
    fn flags_perform_prefix() {
        assert!(run_on("function performAction() {}")
            .iter()
            .any(|d| d.message.contains("performAction")));
    }

    #[test]
    fn flags_data_prefix_compound() {
        assert!(run_on("const dataSource = 1;")
            .iter()
            .any(|d| d.message.contains("dataSource")));
    }

    // --- boundary / false-positive regressions ---

    #[test]
    fn allows_handle_prefix() {
        // `handleXxx` is the canonical React event-handler naming convention
        // (`onClick={handleClick}`). Must not be flagged.
        for name in ["handleClick", "handleSubmit", "handle_change", "handle"] {
            let source = format!("const {name} = () => {{}};");
            assert!(
                run_on(&source).is_empty(),
                "'{name}' must NOT be flagged — `handle` is a React idiom"
            );
        }
    }

    // --- global identifier allowlist ---

    #[test]
    fn allows_process_global_usage() {
        // `process` is a Node global; `process.env.NODE_ENV` must not fire.
        assert!(run_on("const x = process.env.NODE_ENV;").is_empty());
    }

    #[test]
    fn allows_buffer_global() {
        assert!(run_on("const b = Buffer.from('x');").is_empty());
    }

    #[test]
    fn allows_global_this_global() {
        assert!(run_on("const g = globalThis.something;").is_empty());
    }

    #[test]
    fn allows_console_require_module_exports() {
        assert!(run_on("console.log('x');").is_empty());
        assert!(run_on("const fs = require('fs');").is_empty());
        assert!(run_on("module.exports = {};").is_empty());
    }

    #[test]
    fn allows_dirname_filename_globals() {
        assert!(run_on("const p = __dirname;").is_empty());
        assert!(run_on("const f = __filename;").is_empty());
    }

    #[test]
    fn still_flags_derived_process_names() {
        // Derived names still hit the prefix rule — only the exact global
        // name `process` is exempted. `processor` has no word boundary so
        // it's also allowed (no banned-prefix match).
        assert!(run_on("const processOrder = 1;")
            .iter()
            .any(|d| d.message.contains("processOrder")));
        assert!(run_on("const process_order = 1;")
            .iter()
            .any(|d| d.message.contains("process_order")));
    }

    #[test]
    fn does_not_flag_word_with_prefix_letters() {
        for name in [
            "document", "database", "domain", "handler", "dataset",
            "performance", "runtime",
        ] {
            let source = format!("const {name} = 5;");
            assert!(
                run_on(&source).is_empty(),
                "'{name}' must NOT be flagged — no word boundary"
            );
        }
    }

    #[test]
    fn allows_destructured_data_from_api() {
        assert!(run_on("const { data } = useQuery();").is_empty());
        assert!(run_on("const { data, error } = await authClient.signIn();").is_empty());
    }

    #[test]
    fn does_not_flag_screaming_snake_with_prefix_substring() {
        assert!(run_on("const DATABASE_ERROR = 1;").is_empty(),
            "DATABASE_ERROR must not match prefix 'data'");
    }

    #[test]
    fn still_flags_screaming_snake_with_real_boundary() {
        assert!(!run_on("const DATA_SOURCE = 1;").is_empty(),
            "DATA_SOURCE should flag — DATA + _ is a word boundary");
    }

    #[test]
    fn allows_data_as_object_literal_key() {
        assert!(run_on("return { data: session };").is_empty());
        assert!(run_on("const x = { data: 1 };").is_empty());
    }

    #[test]
    fn allows_method_call_with_banned_prefix() {
        assert!(run_on("db.execute(sql)").is_empty());
        assert!(run_on("runner.performTask()").is_empty());
        assert!(run_on("queue.processMessage(msg)").is_empty());
    }

    #[test]
    fn allows_property_definition_in_object_literal() {
        // Object literal keys often conform to an interface — skip them.
        assert!(run_on("const x = { processOrder: fn };").is_empty());
    }

    #[test]
    fn still_flags_variable_with_banned_prefix() {
        assert!(!run_on("const processOrder = 1;").is_empty());
    }
}
