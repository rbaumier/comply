//! rust-unsafe-impl-without-comment backend.
//!
//! Walks `impl_item` nodes and flags any whose source text starts
//! with `unsafe impl` and that has no `// SAFETY:` comment on the
//! lines directly above. Same scan-upward logic as
//! `rust-undocumented-unsafe`: skip blanks and other comments, stop
//! at the first real code line.
//!
//! A single `// SAFETY:` comment covers a contiguous run of `unsafe
//! impl` items (the idiomatic `Send` + `Sync` pairing): an `unsafe
//! impl` with no comment of its own inherits coverage when an earlier
//! `unsafe impl` in its run carries one. The run breaks at any
//! non-`unsafe impl` item, so an impl that does not follow a covered
//! run still flags.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::has_adjacent_safety_comment;

const KINDS: &[&str] = &["impl_item"];

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
        if !is_unsafe_impl(node, source_bytes) {
            return;
        }
        if has_adjacent_safety_comment(node, ctx.source) {
            return;
        }
        if run_is_covered(node, ctx.source) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-unsafe-impl-without-comment".into(),
            message: "`unsafe impl` without a `// SAFETY:` comment — \
                      spell out which invariants of the unsafe trait \
                      the type upholds. The comment is the entire \
                      audit trail for the unsafe contract."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True if `node` is an `unsafe impl` item. tree-sitter-rust doesn't expose
/// `unsafe` as a named child on `impl_item`, so we check the node's source
/// prefix.
fn is_unsafe_impl(node: tree_sitter::Node, source_bytes: &[u8]) -> bool {
    node.utf8_text(source_bytes)
        .is_ok_and(|text| text.trim_start().starts_with("unsafe impl"))
}

/// True if `node` inherits SAFETY coverage from an earlier `unsafe impl` in its
/// contiguous run. A `// SAFETY:` comment above the first impl of a run of
/// consecutive `unsafe impl` items covers every impl in that run (the idiomatic
/// `Send` + `Sync` pairing sharing one justification).
///
/// Walks back over preceding `unsafe impl` siblings (skipping interleaved
/// comment siblings); the first non-`unsafe impl` item breaks the run. If any
/// earlier impl in the run carries its own SAFETY comment, `node` is covered.
fn run_is_covered(node: tree_sitter::Node, source: &str) -> bool {
    let source_bytes = source.as_bytes();
    let mut sibling = node.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "line_comment" | "block_comment" => {}
            "impl_item" if is_unsafe_impl(s, source_bytes) => {
                if has_adjacent_safety_comment(s, source) {
                    return true;
                }
            }
            _ => return false,
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
    fn flags_bare_unsafe_impl_send() {
        let source = "struct Foo;\nunsafe impl Send for Foo {}";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_unsafe_impl_with_safety_comment() {
        let source = "struct Foo;\n\
                      // SAFETY: Foo holds only Send fields, so the type is itself Send.\n\
                      unsafe impl Send for Foo {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_safe_impl() {
        let source = "struct Foo;\nimpl Display for Foo { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { Ok(()) } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn one_safety_comment_covers_consecutive_send_sync() {
        let source = "struct OpCodeInfo;\n\
                      // SAFETY: The `NonNull` is just a `&'static str`.\n\
                      unsafe impl Send for OpCodeInfo {}\n\
                      unsafe impl Sync for OpCodeInfo {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn run_coverage_spans_blank_lines() {
        let source = "struct Foo;\n\
                      // SAFETY: Foo is only shared behind a lock.\n\
                      unsafe impl Send for Foo {}\n\
                      \n\
                      unsafe impl Sync for Foo {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn second_impl_still_flags_when_run_has_no_comment() {
        let source = "struct Foo;\n\
                      unsafe impl Send for Foo {}\n\
                      unsafe impl Sync for Foo {}";
        assert_eq!(run_on(source).len(), 2);
    }

    #[test]
    fn non_unsafe_impl_between_breaks_the_run() {
        let source = "struct Foo;\n\
                      // SAFETY: Foo is only shared behind a lock.\n\
                      unsafe impl Send for Foo {}\n\
                      impl Foo { fn bar(&self) {} }\n\
                      unsafe impl Sync for Foo {}";
        let diags = run_on(source);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 5);
    }

    #[test]
    fn non_safety_comment_does_not_cover_the_run() {
        let source = "struct Foo;\n\
                      // TODO: prove these are sound.\n\
                      unsafe impl Send for Foo {}\n\
                      unsafe impl Sync for Foo {}";
        assert_eq!(run_on(source).len(), 2);
    }
}
