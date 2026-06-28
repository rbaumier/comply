//! rust-prefer-unwrap-or-explicit backend.
//!
//! Flags `.unwrap_or_default()` method calls in non-test code. The
//! reader should be able to tell, at the call site, what value is
//! produced on `None`/`Err` without having to look up the `Default`
//! impl of the receiver's type. `.unwrap_or(<value>)` and
//! `.unwrap_or_else(|| <expr>)` both make the fallback visible; this
//! rule nudges authors toward one of those two forms.
//!
//! Bare `.unwrap()` / `.expect(...)` are intentionally out of scope —
//! they are handled by `rust-no-unwrap`. The two rules are independent
//! and cumulable.
//!
//! `.map(|x| x.is_*()).unwrap_or_default()` is exempt: an `is_`-prefixed
//! method is a Rust-convention boolean predicate, so the map produces a
//! `bool` and the fallback is the universally-known `false`. The
//! "make the fallback visible" goal is already met without an explicit
//! value.
//!
//! Tests are exempted via `is_in_test_context`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_in_test_context;

const KINDS: &[&str] = &["call_expression"];

/// Returns true when `receiver` is `<expr>.map(|x| x.is_*())`, i.e. a
/// `.map(...)` whose closure body is an `is_`-prefixed method call.
///
/// Such a map yields a `bool` (the `is_*` self-convention enforces a
/// `bool` return), so a following `.unwrap_or_default()` produces the
/// universally-known `false` — the fallback is already visible.
///
/// Pure tree-sitter shape check; no type resolution. Only ever
/// suppresses a diagnostic, so a misnamed `is_*` returning non-bool
/// merely yields a harmless false negative, never a false positive.
fn is_bool_predicate_map(receiver: tree_sitter::Node, source_bytes: &[u8]) -> bool {
    // `receiver` must be `<expr>.map(<closure>)`.
    if receiver.kind() != "call_expression" {
        return false;
    }
    let Some(map_fn) = receiver.child_by_field_name("function") else {
        return false;
    };
    if map_fn.kind() != "field_expression" {
        return false;
    }
    let Some(map_field) = map_fn.child_by_field_name("field") else {
        return false;
    };
    let Ok(map_field_text) = map_field.utf8_text(source_bytes) else {
        return false;
    };
    if map_field_text != "map" {
        return false;
    }
    // The argument must be a closure.
    let Some(args) = receiver.child_by_field_name("arguments") else {
        return false;
    };
    let mut cursor = args.walk();
    let Some(closure) = args
        .named_children(&mut cursor)
        .find(|c| c.kind() == "closure_expression")
    else {
        return false;
    };
    let Some(body) = closure.child_by_field_name("body") else {
        return false;
    };
    // Resolve a block-bodied closure (`|x| { x.is_dir() }`) to its tail
    // expression. A bare expression body (`|x| x.is_dir()`) is used as-is.
    let effective = if body.kind() == "block" {
        let mut block_cursor = body.walk();
        match body.named_children(&mut block_cursor).last() {
            Some(tail) => tail,
            None => return false,
        }
    } else {
        body
    };
    // The body must be a method call `<x>.is_*()`.
    if effective.kind() != "call_expression" {
        return false;
    }
    let Some(callee) = effective.child_by_field_name("function") else {
        return false;
    };
    if callee.kind() != "field_expression" {
        return false;
    }
    let Some(method) = callee.child_by_field_name("field") else {
        return false;
    };
    let Ok(method_name) = method.utf8_text(source_bytes) else {
        return false;
    };
    method_name
        .strip_prefix("is_")
        .is_some_and(|rest| !rest.is_empty())
}

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
        // Looking for `receiver.unwrap_or_default()`.
        let Some(function) = node.child_by_field_name("function") else {
            return;
        };
        if function.kind() != "field_expression" {
            return;
        }
        let Some(field) = function.child_by_field_name("field") else {
            return;
        };
        let Ok(field_text) = field.utf8_text(source_bytes) else {
            return;
        };
        if field_text != "unwrap_or_default" {
            return;
        }
        if is_in_test_context(node, source_bytes) {
            return;
        }
        // `.map(|x| x.is_*()).unwrap_or_default()` maps to a `bool`; its
        // default (`false`) is universally known, so the fallback is
        // already visible — exempt it.
        if let Some(receiver) = function.child_by_field_name("value") {
            if is_bool_predicate_map(receiver, source_bytes) {
                return;
            }
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-prefer-unwrap-or-explicit".into(),
            message: "`.unwrap_or_default()` hides the fallback value from the reader. \
                      Write it explicitly: `.unwrap_or(<value>)` or \
                      `.unwrap_or_else(|| <expr>)`. The goal is that a reader should \
                      see what the code does on None/Err without looking up trait impls."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
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
    fn flags_unwrap_or_default() {
        assert_eq!(run_on("fn f() { let _ = x.unwrap_or_default(); }").len(), 1);
    }

    #[test]
    fn allows_unwrap_or_explicit() {
        assert!(run_on("fn f() { let _ = x.unwrap_or(0); }").is_empty());
    }

    #[test]
    fn allows_unwrap_or_else() {
        assert!(run_on("fn f() { let _ = x.unwrap_or_else(|| 0); }").is_empty());
    }

    #[test]
    fn does_not_flag_plain_unwrap() {
        assert!(run_on("fn f() { let _ = x.unwrap(); }").is_empty());
    }

    #[test]
    fn allows_in_test_context() {
        let source = "#[test]\nfn t() { let _ = x.unwrap_or_default(); }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unrelated_method() {
        assert!(run_on("fn f() { let _ = x.default(); }").is_empty());
    }

    #[test]
    fn allows_is_predicate_map() {
        // Issue #6608 repro: `resolver(path).map(|m| m.is_dir())` is
        // `Result<bool, _>`, so `.unwrap_or_default()` is the obvious
        // `false`.
        let source = "fn f() -> bool { r.map(|metadata| metadata.is_dir()).unwrap_or_default() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_is_predicate_map_block_body() {
        let source = "fn f() -> bool { r.map(|m| { m.is_dir() }).unwrap_or_default() }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_non_is_predicate_map() {
        // Mapped method is not an `is_*` predicate (returns usize), so the
        // default is the non-obvious case the rule targets.
        let source = "fn f() { let _ = r.map(|x| x.len()).unwrap_or_default(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_to_owned_map() {
        let source = "fn f() { let _ = r.map(|x| x.to_owned()).unwrap_or_default(); }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_bare_unwrap_or_default() {
        // No `.map()` receiver at all — still flags.
        assert_eq!(run_on("fn f() { let _ = opt.unwrap_or_default(); }").len(), 1);
    }
}
