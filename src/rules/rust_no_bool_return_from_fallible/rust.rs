//! rust-no-bool-return-from-fallible backend.
//!
//! Walks `function_item` nodes whose return type is `bool` and whose
//! name suggests an action (verb prefix from a small allowlist). The
//! smell is an action whose boolean outcome is a hardcoded `true` /
//! `false` literal: the operation ran but its failure reason is
//! collapsed into a bare bool the caller can't act on.
//!
//! Three shapes are exempted because the bool is legitimate:
//! - Pure predicates (`is_empty`, `has_x`, `contains`): a bool is the
//!   right answer to a question about state.
//! - Trait-impl methods (`impl Trait for Type`): the signature is fixed
//!   by the trait contract, the implementor can't return `Result`.
//! - Functions whose body tail expression *computes* the bool from a
//!   real value — a comparison (parser progress: `pos() != start`) or a
//!   forwarded call return (`HashSet::insert`'s "was it new?") — rather
//!   than hardcoding a literal.

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
    // A block's implicit return is its last named child, provided that
    // child is an expression: a trailing statement ends in `;` and is
    // wrapped in `expression_statement`, so it is not a tail value.
    let mut cursor = body.walk();
    let Some(tail) = body
        .named_children(&mut cursor)
        .filter(|child| child.kind() != "line_comment" && child.kind() != "block_comment")
        .last()
    else {
        return false;
    };
    matches!(
        tail.kind(),
        "binary_expression" | "call_expression" | "await_expression" | "try_expression"
    )
}

fn looks_like_action(name: &str) -> bool {
    let lower = format!("{}_", name.to_ascii_lowercase());
    ACTION_PREFIXES.iter().any(|p| lower.starts_with(p))
}

fn looks_like_predicate(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    EXEMPT_PREFIXES.iter().any(|p| lower.starts_with(p))
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
}
