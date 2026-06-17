//! rust-no-bool-return-from-fallible backend.
//!
//! Walks `function_item` nodes whose return type is `bool` and whose
//! name suggests an action (verb prefix from a small allowlist). The
//! smell is an action whose boolean outcome is a hardcoded `true` /
//! `false` literal: the operation ran but its failure reason is
//! collapsed into a bare bool the caller can't act on.
//!
//! Several shapes are exempted because the bool is legitimate:
//! - Pure predicates (`is_empty`, `has_x`, `contains`): a bool is the
//!   right answer to a question about state.
//! - Atomic fetch/bitwise ops (`fetch_and`, `fetch_or`, `fetch_xor`,
//!   `fetch_nand`, …): the bool is the *previous* atomic value, not a
//!   success flag — these always succeed.
//! - Functions with a leading doc comment phrased as a boolean
//!   question/answer ("Returns `true` if …", "Checks whether …"): the
//!   doc marks a predicate even when the name carries an action prefix
//!   (e.g. seqlock `validate_read`).
//! - Trait-impl methods (`impl Trait for Type`): the signature is fixed
//!   by the trait contract, the implementor can't return `Result`.
//! - Functions whose body tail expression *computes* the bool from a
//!   real value — a comparison (parser progress: `pos() != start`) or a
//!   forwarded call return (`HashSet::insert`'s "was it new?") — rather
//!   than hardcoding a literal. A `match`/`if` tail counts as computed
//!   when at least one branch body forwards a computed value (e.g.
//!   `match { Some(f) => (f)(x), None => true }`); a `match`/`if` whose
//!   every branch is a bare `true` / `false` is still the smell.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_in_trait_impl;

const KINDS: &[&str] = &["function_item"];

const ACTION_PREFIXES: &[&str] = &[
    "save_",
    "delete_",
    "remove_",
    "create_",
    "update_",
    "insert_",
    "parse_",
    "validate_",
    "connect_",
    "send_",
    "write_",
    "load_",
    "execute_",
    "process_",
    "publish_",
    "submit_",
    "commit_",
    "apply_",
    "fetch_",
    "store_",
    "register_",
    "unregister_",
];

const EXEMPT_PREFIXES: &[&str] = &[
    "is_",
    "has_",
    "should_",
    "can_",
    "may_",
    "must_",
    "needs_",
    "contains_",
    "matches_",
    "supports_",
    "accepts_",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let Ok(name) = name_node.utf8_text(source_bytes) else {
            return;
        };
        if !looks_like_action(name) {
            return;
        }
        // Predicate-style names take precedence: an `is_valid()` that
        // returns bool is correct, even if it's also "save_valid".
        if looks_like_predicate(name) {
            return;
        }
        // Atomic fetch/bitwise ops (`fetch_and`, `fetch_or`, `fetch_xor`,
        // `fetch_nand`, …) return the *previous* atomic value, not a
        // success flag — they always succeed. Mirrors
        // `std::sync::atomic::AtomicBool::fetch_*`.
        if is_atomic_fetch_op(name) {
            return;
        }
        // A leading doc comment phrased as a boolean question/answer
        // ("Returns `true` if …", "Checks whether …") marks the function
        // as a predicate even when its name carries an action prefix
        // (e.g. seqlock `validate_read`).
        if has_predicate_doc_comment(node, source_bytes) {
            return;
        }
        let Some(ret_type) = node.child_by_field_name("return_type") else {
            return;
        };
        let Ok(ret_text) = ret_type.utf8_text(source_bytes) else {
            return;
        };
        if ret_text.trim() != "bool" {
            return;
        }
        // A trait-impl method can't change its signature to `Result` —
        // the `bool` is dictated by the trait contract.
        if is_in_trait_impl(node) {
            return;
        }
        // The bool is a genuine computed value (parser-progress
        // comparison, forwarded collection-insert result), not a
        // failure smuggled as a hardcoded `true` / `false`.
        if returns_computed_bool(node) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-no-bool-return-from-fallible".into(),
            message: format!(
                "`fn {name}(..) -> bool` — action functions must \
                 return `Result<T, E>` so the caller can see why \
                 the operation failed. Use `Result<(), MyError>` \
                 if there's no success payload."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

/// True if the function's body tail expression *computes* its boolean
/// result from a real value rather than hardcoding `true` / `false`.
///
/// The rule's smell is an action that discards an operation's failure
/// and returns a bare literal. When the implicit-return expression is a
/// comparison (`pos() != start` — parser progress) or a directly
/// forwarded call return (`self.set.insert(x)` — was it new?), the bool
/// carries the operation's actual outcome and is the right return type.
fn returns_computed_bool(func: tree_sitter::Node) -> bool {
    let Some(body) = func.child_by_field_name("body") else {
        return false;
    };
    let Some(tail) = block_tail_expression(body) else {
        return false;
    };
    expression_is_computed(tail)
}

/// A block's implicit return is its last named child, provided that child
/// is an expression. A trailing `expr;` statement is wrapped in
/// `expression_statement`; the `;` is a separate `empty_statement`, so a
/// genuine tail statement leaves that `empty_statement` last and is not
/// mistaken for a value. Block-like tail expressions (`match`, `if`) are
/// themselves wrapped in `expression_statement` even without a `;`, so the
/// wrapper is unwrapped to expose the inner expression. Comments are
/// skipped.
fn block_tail_expression(block: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut cursor = block.walk();
    let tail = block
        .named_children(&mut cursor)
        .filter(|child| child.kind() != "line_comment" && child.kind() != "block_comment")
        .last()?;
    if tail.kind() == "expression_statement" {
        return tail.named_child(0);
    }
    Some(tail)
}

/// True if `expr` *computes* its boolean value from a real value rather
/// than hardcoding `true` / `false`. A comparison (`pos() != start`) or a
/// forwarded call return (`self.set.insert(x)`, a closure's `(f)(x)`)
/// carries the operation's actual outcome. A `match`/`if` is computed iff
/// at least one branch body is itself computed — an all-literal `match`/`if`
/// (`if ok { true } else { false }`) is the genuine literal-smuggling smell
/// and is not treated as computed.
fn expression_is_computed(expr: tree_sitter::Node) -> bool {
    match expr.kind() {
        "binary_expression" | "call_expression" | "await_expression" | "try_expression" => true,
        "match_expression" => match_has_computed_arm(expr),
        "if_expression" => if_has_computed_branch(expr),
        _ => false,
    }
}

/// True if any `match_arm`'s body expression is computed. The arm body is
/// the arm's `value` field, which is either a bare expression or a `block`
/// whose tail is the value.
fn match_has_computed_arm(match_expr: tree_sitter::Node) -> bool {
    let Some(body) = match_expr.child_by_field_name("body") else {
        return false;
    };
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .filter(|child| child.kind() == "match_arm")
        .filter_map(|arm| arm.child_by_field_name("value"))
        .any(branch_body_is_computed)
}

/// True if any branch of an `if`/`else` chain computes its tail value. The
/// consequent is a `block`; the alternative is an `else_clause` wrapping a
/// `block` or a chained `else if` (`if_expression`).
fn if_has_computed_branch(if_expr: tree_sitter::Node) -> bool {
    if let Some(cons) = if_expr.child_by_field_name("consequence")
        && branch_body_is_computed(cons)
    {
        return true;
    }
    let Some(alt) = if_expr.child_by_field_name("alternative") else {
        return false;
    };
    match alt.kind() {
        // `else if`: the alternative is directly another `if_expression`.
        "if_expression" => if_has_computed_branch(alt),
        // `else { .. }`: the `else_clause` wraps a `block` or a chained
        // `else if` (`if_expression`).
        "else_clause" => {
            let mut cursor = alt.walk();
            alt.named_children(&mut cursor)
                .any(|child| match child.kind() {
                    "if_expression" => if_has_computed_branch(child),
                    "block" => branch_body_is_computed(child),
                    _ => false,
                })
        }
        _ => false,
    }
}

/// True if a branch body computes its value. A `block` is unwrapped to its
/// tail expression; any other node is the branch value directly (a bare
/// arm body). Recurses through nested `match`/`if`.
fn branch_body_is_computed(body: tree_sitter::Node) -> bool {
    let expr = if body.kind() == "block" {
        match block_tail_expression(body) {
            Some(tail) => tail,
            None => return false,
        }
    } else {
        body
    };
    expression_is_computed(expr)
}

fn looks_like_action(name: &str) -> bool {
    let lower = format!("{}_", name.to_ascii_lowercase());
    ACTION_PREFIXES.iter().any(|p| lower.starts_with(p))
}

fn looks_like_predicate(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    EXEMPT_PREFIXES.iter().any(|p| lower.starts_with(p))
}

/// True if `name` is an atomic fetch/bitwise op whose `bool` return is the
/// prior atomic value (`fetch`, `fetch_and`, `fetch_or`, `fetch_xor`,
/// `fetch_nand`, …). These mirror `std::sync::atomic::AtomicBool::fetch_*`
/// and always succeed, so the `bool` payload is correct.
fn is_atomic_fetch_op(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower == "fetch" || lower.starts_with("fetch_")
}

/// True if a doc comment immediately preceding the function reads as a
/// boolean question/answer — i.e. the function is a predicate, not a
/// fallible action, even though its name carries an action prefix.
///
/// In tree-sitter-rust, doc comments (`///`, `/** */`) are `line_comment`
/// / `block_comment` siblings preceding the item, possibly interleaved
/// with `attribute_item`s. The match is on the leading phrase only, so an
/// action that merely mentions "validate" in prose still fires.
fn has_predicate_doc_comment(func: tree_sitter::Node, source: &[u8]) -> bool {
    const PREDICATE_DOC_LEADS: &[&str] = &[
        "returns `true`",
        "returns true",
        "returns whether",
        "checks whether",
        "returns `false`",
        "returns false",
    ];
    let mut sibling = func.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "attribute_item" => {}
            "line_comment" | "block_comment" => {
                if let Ok(text) = s.utf8_text(source) {
                    let normalized = text
                        .trim_start_matches(['/', '*', '!'])
                        .trim()
                        .to_ascii_lowercase();
                    if PREDICATE_DOC_LEADS
                        .iter()
                        .any(|lead| normalized.starts_with(lead))
                    {
                        return true;
                    }
                }
            }
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
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
    fn flags_save_returning_bool() {
        assert_eq!(run_on("fn save_user(u: &User) -> bool { true }").len(), 1);
    }

    #[test]
    fn flags_parse_returning_bool() {
        assert_eq!(run_on("fn parse_config(s: &str) -> bool { true }").len(), 1);
    }

    #[test]
    fn allows_save_returning_result() {
        assert!(run_on("fn save_user(u: &User) -> Result<(), MyError> { Ok(()) }").is_empty());
    }

    #[test]
    fn allows_predicate_is_valid() {
        assert!(run_on("fn is_valid(s: &str) -> bool { true }").is_empty());
    }

    #[test]
    fn allows_predicate_has_permission() {
        assert!(run_on("fn has_permission(u: &User) -> bool { true }").is_empty());
    }

    #[test]
    fn does_not_flag_unrelated_function() {
        assert!(run_on("fn add(a: i32, b: i32) -> i32 { a + b }").is_empty());
    }

    // --- #1733: trait-impl methods (signature fixed by the contract) ---

    #[test]
    fn allows_validate_in_trait_impl() {
        let src = "\
            impl biome_deserialize::DeserializableValidator for FilenameCases {\n\
                fn validate(&mut self, ctx: &mut C, name: &str, range: R) -> bool {\n\
                    if !self.allow_export && self.cases.is_empty() {\n\
                        ctx.report(d);\n\
                        false\n\
                    } else {\n\
                        true\n\
                    }\n\
                }\n\
            }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn flags_validate_in_inherent_impl() {
        // An inherent (non-trait) impl can freely return `Result`, so the
        // bool smell still applies.
        let src = "impl Foo { fn validate_thing(&self) -> bool { true } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- #1733: parser-progress comparison return ---

    #[test]
    fn allows_parser_progress_comparison() {
        let src = "\
            pub(crate) fn parse_css_generic_component_value_list(p: &mut TailwindParser) -> bool {\n\
                let start = p.source().position();\n\
                CssGenericComponentValueList.parse_list(p);\n\
                p.source().position() != start\n\
            }";
        assert!(run_on(src).is_empty());
    }

    // --- #1733: forwarded collection-insert result ---

    #[test]
    fn allows_forwarded_insert_result() {
        let src = "\
            fn insert_watched_folder(&self, path: Utf8PathBuf) -> bool {\n\
                self.watched_folders.pin().insert(path)\n\
            }";
        assert!(run_on(src).is_empty());
    }

    // --- #1733: negative space — genuine fallible action still flagged ---

    #[test]
    fn flags_genuine_action_swallowing_failure() {
        // The side effect runs as a statement and the outcome is a
        // hardcoded literal — this is exactly the smell.
        let src = "fn save_user(u: &User) -> bool { db.write(u); true }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_genuine_action_with_literal_branches() {
        let src = "fn save_user(u: &User) -> bool { if ok { true } else { false } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- #3965: match/if tail whose arm forwards a computed bool ---

    #[test]
    fn allows_validate_match_forwarding_closure_bool() {
        // async-graphql `ScalarType::validate` — the `Some` arm forwards a
        // user closure's `bool`; `None` legitimately means "valid".
        let src = "\
            pub(crate) fn validate(&self, value: &Value) -> bool {\n\
                match &self.validator {\n\
                    Some(validator) => (validator)(value),\n\
                    None => true,\n\
                }\n\
            }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_if_tail_forwarding_call_in_one_branch() {
        let src = "\
            fn validate_thing(&self, cond: bool) -> bool {\n\
                if cond { self.compute() } else { true }\n\
            }";
        assert!(run_on(src).is_empty());
    }

    // --- #3965: negative space — all-literal branch bodies still fire ---

    #[test]
    fn flags_match_with_all_literal_arms() {
        let src = "fn validate_state(&self, x: State) -> bool { match x { A => true, B => false } }";
        assert_eq!(run_on(src).len(), 1);
    }

    // --- #1479: atomic `fetch_*` ops return the previous value, not success ---

    #[test]
    fn allows_atomic_fetch_and() {
        // crossbeam-utils AtomicCell<bool>::fetch_and — the bool is the
        // previous atomic value (the body tail is a macro invocation, so
        // `returns_computed_bool` cannot see through it).
        let src = "\
            pub fn fetch_and(&self, val: bool) -> bool {\n\
                atomic! {\n\
                    bool, _a,\n\
                    { a.fetch_and(val, Ordering::AcqRel) },\n\
                    { let old = *value; *value &= val; old }\n\
                }\n\
            }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_atomic_fetch_or() {
        // Literal tail so the exemption comes from the name class, not
        // from `returns_computed_bool`.
        let src = "pub fn fetch_or(&self, val: bool) -> bool { side_effect(); true }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_atomic_fetch_xor() {
        let src = "pub fn fetch_xor(&self, val: bool) -> bool { side_effect(); true }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_atomic_fetch_nand() {
        let src = "pub fn fetch_nand(&self, val: bool) -> bool { side_effect(); true }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_bare_fetch_returning_prior_bool() {
        let src = "pub fn fetch(&self, val: bool) -> bool { side_effect(); true }";
        assert!(run_on(src).is_empty());
    }

    // --- #1479: predicate functions whose doc says "Returns `true`" ---

    #[test]
    fn allows_validate_read_with_returns_true_doc() {
        // crossbeam-utils SeqLock::validate_read — a pure predicate
        // ("Returns `true` if the current stamp is equal to `stamp`.").
        let src = "\
            /// Returns `true` if the current stamp is equal to `stamp`.\n\
            pub(crate) fn validate_read(&self, stamp: (usize, usize)) -> bool {\n\
                some_side_effect();\n\
                true\n\
            }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_check_prefix_with_returns_whether_doc() {
        let src = "\
            /// Returns whether the cache is consistent.\n\
            fn check_consistency(&self) -> bool { do_thing(); true }";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_validate_prefix_with_checks_whether_doc() {
        let src = "\
            /// Checks whether the signature is well-formed.\n\
            fn validate_signature(&self) -> bool { do_thing(); true }";
        assert!(run_on(src).is_empty());
    }

    // --- #1479: negative space — `validate_`/`check_` without a predicate
    // doc comment is still a genuine fallible action and must fire ---

    #[test]
    fn flags_validate_action_without_predicate_doc() {
        let src = "fn validate_config(&self) -> bool { do_thing(); true }";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_validate_action_with_unrelated_doc() {
        let src = "\
            /// Validates the config and persists the result.\n\
            fn validate_config(&self) -> bool { do_thing(); true }";
        assert_eq!(run_on(src).len(), 1);
    }
}
