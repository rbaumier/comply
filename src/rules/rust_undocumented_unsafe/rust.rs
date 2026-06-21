//! rust-undocumented-unsafe backend.
//!
//! Flags `unsafe { ... }` blocks that are not preceded by a
//! `// SAFETY: ...` comment explaining the invariants being upheld.
//! Every `unsafe` block is a promise the author makes to the compiler;
//! a code comment is how that promise is documented for reviewers and
//! for future debugging when memory corruption shows up.
//!
//! This rule is equivalent to `clippy::undocumented_unsafe_blocks`,
//! which is in the restriction group and off by default. Running it
//! via comply means consuming crates don't have to opt in — every
//! `unsafe` block in the project must carry its safety justification.
//!
//! Test code is exempt: both by a `tests/` directory (`skip_in_test_dir`)
//! and by an inline `#[test]` / `#[cfg(test)]` context detected via
//! `is_in_test_context`, so unit tests written next to the code they
//! exercise are treated the same as tests under `tests/`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::is_in_test_context;

const KINDS: &[&str] = &["unsafe_block"];

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
        if inside_unsafe_fn(node, ctx.source.as_bytes()) {
            return;
        }
        if is_in_test_context(node, ctx.source.as_bytes()) {
            return;
        }
        if has_safety_comment_above(node, ctx.source) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-undocumented-unsafe".into(),
            message: "`unsafe` block without a `// SAFETY:` comment. \
                      Explain which invariants you're upholding — \
                      future debuggers (including you) will need \
                      that justification when memory corruption hits."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

fn inside_unsafe_fn(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cur = node.parent();
    while let Some(p) = cur {
        if p.kind() == "function_item" {
            let body_start = p
                .child_by_field_name("body")
                .map(|b| b.start_byte())
                .unwrap_or(p.end_byte());
            let sig = &source[p.start_byte()..body_start];
            return sig.windows(6).any(|w| w == b"unsafe");
        }
        cur = p.parent();
    }
    false
}

/// True if a `// SAFETY:` comment documents this unsafe block, either
/// directly above it or above an enclosing `impl`/`fn` whose scope wraps
/// it. The latter covers the convention of documenting a shared invariant
/// once above an `impl` block whose methods all perform the same unsafe
/// operation, instead of repeating the comment above each inner block.
/// One such comment therefore suppresses every unsafe block in the impl's
/// methods — the granularity is the enclosing scope, not the block.
///
/// We check the lines directly above the unsafe block, then walk up the
/// ancestor chain and check the lines directly above each enclosing
/// `impl_item` / `function_item`. Leakage from an unrelated sibling item
/// is prevented by the per-row scan stopping at the first real-code line
/// above each ancestor: a sibling's comment is never directly above an
/// ancestor's start row. The walk bound (`source_file`) only caps how far
/// up the chain we look.
fn has_safety_comment_above(node: tree_sitter::Node, source: &str) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    if safety_comment_above_row(node.start_position().row, &lines) {
        return true;
    }
    let mut cur = node.parent();
    while let Some(p) = cur {
        if matches!(p.kind(), "impl_item" | "function_item")
            && safety_comment_above_row(p.start_position().row, &lines)
        {
            return true;
        }
        cur = p.parent();
    }
    false
}

/// True if a `// SAFETY:` comment sits on the lines directly above
/// `start_row`. We scan by text (the comment may be on any of the
/// preceding lines up to the previous code line) because tree-sitter
/// doesn't attach comments to expressions. Blank lines, other comments,
/// and outer attributes (`#[...]`) are skipped so a comment above an
/// `impl` carrying `#[allow(unsafe_code)]` still counts. A contiguous run
/// of simple `let` bindings directly above is also skipped: documenting
/// the invariant once above the preparatory bindings, then performing the
/// unsafe call, is idiomatic. The scan stops at the first non-`let` code
/// line (a call/expression statement, an opening/closing brace, another
/// unsafe block), so a stray faraway comment never counts.
fn safety_comment_above_row(start_row: usize, lines: &[&str]) -> bool {
    let mut row = start_row;
    while row > 0 {
        row -= 1;
        let Some(line) = lines.get(row) else { break };
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("//") || trimmed.starts_with("/*") {
            if trimmed.contains("SAFETY:") || trimmed.contains("Safety:") || trimmed.contains("# Safety") {
                return true;
            }
            continue;
        }
        if is_simple_let_binding(trimmed) {
            // Preparatory binding between the comment and the unsafe block —
            // skip it and keep looking upward for the SAFETY comment.
            continue;
        }
        // Hit real code — stop looking.
        break;
    }
    false
}

/// True if `trimmed` is a complete single-line `let` binding, e.g.
/// `let handler = setup();`. Requires the `let` keyword at the start and a
/// trailing `;` so a multi-line binding's continuation lines (which carry
/// arbitrary code) don't get skipped, and a binding initialized from its
/// own `unsafe` block (`let x = unsafe { .. };`) is not treated as plain
/// setup.
fn is_simple_let_binding(trimmed: &str) -> bool {
    let Some(rest) = trimmed.strip_prefix("let ") else {
        return false;
    };
    trimmed.ends_with(';') && !rest.contains("unsafe")
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
    fn flags_bare_unsafe_block() {
        let source = "fn f(p: *const u8) { unsafe { let _ = *p; } }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn exempt_in_test_dir_issue_1011() {
        // Issue #1011: sled tests/test_crash_recovery.rs — bare unsafe in a
        // test file. skip_in_test_dir suppresses the rule under tests/.
        let source = "fn f() { unsafe { env::set_var(\"K\", \"v\"); } }";
        // Bare unsafe block still flags on a normal source path.
        assert_eq!(
            crate::rules::test_helpers::run_rule_gated(&Check, source, "src/lib.rs").len(),
            1
        );
        // …but is exempt under a tests/ directory.
        assert!(
            crate::rules::test_helpers::run_rule_gated(&Check, source, "tests/test_crash_recovery.rs")
                .is_empty()
        );
    }

    #[test]
    fn allows_unsafe_with_safety_comment() {
        let source = "fn f(p: *const u8) {\n\
                      // SAFETY: p is non-null and points to valid memory.\n\
                      unsafe { let _ = *p; }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_unsafe_with_multi_line_comment() {
        let source = "fn f(p: *const u8) {\n\
                      // SAFETY: caller guarantees non-null.\n\
                      //         See the docs on this function.\n\
                      unsafe { let _ = *p; }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_unsafe_fn_declaration() {
        assert!(run_on("unsafe fn f() {}").is_empty());
    }

    #[test]
    fn allows_unsafe_block_inside_unsafe_fn() {
        let source = "unsafe fn f(p: *const u8) -> u8 { unsafe { *p } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_rustdoc_safety_heading() {
        let source = "fn f(p: *const u8) {\n\
                      /// # Safety\n\
                      /// p must be valid\n\
                      unsafe { let _ = *p; }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_lowercase_safety_comment() {
        let source = "fn f(p: *const u8) {\n\
                      // Safety: p checked above\n\
                      unsafe { let _ = *p; }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn exempt_inline_test_fn_issue_3890() {
        // Issue #3890: an inline `#[test] fn` in a src/ file with a bare
        // unsafe block (no SAFETY comment) must not be flagged.
        let source = "#[test]\n\
                      fn test_value_eq_value() {\n\
                      unsafe {\n\
                      let _ = from_shared_unchecked(b\"..{}\");\n\
                      }\n\
                      }";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, source, "src/metadata/value.rs").is_empty()
        );
    }

    #[test]
    fn exempt_inline_cfg_test_mod_issue_3890() {
        // The other `is_in_test_context` form: a `#[cfg(test)] mod tests`
        // in a src/ file. A bare unsafe block inside it is exempt.
        let source = "#[cfg(test)]\n\
                      mod tests {\n\
                      fn helper(p: *const u8) {\n\
                      unsafe { let _ = *p; }\n\
                      }\n\
                      }";
        assert!(
            crate::rules::test_helpers::run_rule(&Check, source, "src/metadata/value.rs").is_empty()
        );
    }

    #[test]
    fn allows_impl_block_level_safety_comment_issue_5046() {
        // Issue #5046: indexmap src/map/slice.rs — a single `// SAFETY:`
        // comment above an `impl` block (carrying `#[allow(unsafe_code)]`)
        // documents the shared invariant for every unsafe block its methods
        // contain, instead of repeating it above each one.
        let source = "// SAFETY: `Slice<K, V>` is a transparent wrapper around `[Bucket<K, V>]`.\n\
                      #[allow(unsafe_code)]\n\
                      impl<K, V> Slice<K, V> {\n\
                      pub(crate) const fn from_slice(entries: &[Bucket<K, V>]) -> &Self {\n\
                      unsafe { &*(entries as *const [Bucket<K, V>] as *const Self) }\n\
                      }\n\
                      pub(super) const fn from_mut_slice(entries: &mut [Bucket<K, V>]) -> &mut Self {\n\
                      unsafe { &mut *(entries as *mut [Bucket<K, V>] as *mut Self) }\n\
                      }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_unsafe_in_impl_without_any_safety_comment() {
        // Negative: an unsafe block inside an impl with no SAFETY comment
        // anywhere in its enclosing scope chain still fires (once per block).
        let source = "impl<K, V> Slice<K, V> {\n\
                      pub(crate) const fn from_slice(entries: &[Bucket<K, V>]) -> &Self {\n\
                      unsafe { &*(entries as *const [Bucket<K, V>] as *const Self) }\n\
                      }\n\
                      pub(super) const fn from_mut_slice(entries: &mut [Bucket<K, V>]) -> &mut Self {\n\
                      unsafe { &mut *(entries as *mut [Bucket<K, V>] as *mut Self) }\n\
                      }\n\
                      }";
        assert_eq!(run_on(source).len(), 2);
    }

    #[test]
    fn flags_bare_unsafe_in_non_test_fn() {
        // Production guard: an undocumented unsafe block in an ordinary
        // (non-test) fn at a src/ path still fires.
        let source = "fn f(p: *const u8) {\n\
                      unsafe { let _ = *p; }\n\
                      }";
        assert_eq!(
            crate::rules::test_helpers::run_rule(&Check, source, "src/metadata/value.rs").len(),
            1
        );
    }

    #[test]
    fn allows_safety_comment_separated_by_let_issue_5199() {
        // Issue #5199: miette src/eyreish/error.rs — a `// Safety:` comment
        // documents the unsafe block but a preparatory `let` binding sits
        // between them. The upward scan skips the simple `let` and finds the
        // comment.
        let source = "fn f(error: E) {\n\
                      // Safety: passing vtable that operates on the right type E.\n\
                      let handler = Some(super::capture_handler(&error));\n\
                      unsafe { Report::construct(error, vtable, handler) }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_safety_comment_separated_by_two_lets_issue_5199() {
        // Two contiguous preparatory bindings between the comment and the
        // unsafe block are both skipped.
        let source = "fn f(p: *const u8) {\n\
                      // SAFETY: p is non-null and points to valid memory.\n\
                      let len = compute_len();\n\
                      let cap = len * 2;\n\
                      unsafe { let _ = *p; }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_unsafe_separated_from_comment_by_real_code_issue_5199() {
        // A non-`let` statement (a function call) between the comment and the
        // unsafe block breaks the association — the comment documents the call,
        // not the unsafe block, so it still fires.
        let source = "fn f(p: *const u8) {\n\
                      // SAFETY: p is non-null and points to valid memory.\n\
                      do_setup();\n\
                      unsafe { let _ = *p; }\n\
                      }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_unsafe_with_let_but_no_comment_issue_5199() {
        // Skipping the preparatory `let` must not invent a SAFETY comment:
        // a genuinely undocumented unsafe block above a `let` still fires.
        let source = "fn f(p: *const u8) {\n\
                      let handler = setup();\n\
                      unsafe { let _ = *p; }\n\
                      }";
        assert_eq!(run_on(source).len(), 1);
    }
}
