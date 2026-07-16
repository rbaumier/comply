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
use crate::rules::rust_helpers::{is_in_test_context, is_safety_marker};

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
        if has_safety_comment_above(node, ctx.source)
            || has_safety_comment_inside_block(node, ctx.source)
        {
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
/// directly above it or above an enclosing scope whose comment covers it.
/// Three enclosing-scope conventions are recognized:
///
///  - above an enclosing `impl`/`fn`: documenting a shared invariant once
///    above an `impl` block whose methods all perform the same unsafe
///    operation, instead of repeating the comment above each inner block;
///  - at the start of an enclosing *loop* body (`while`/`loop`/`for`) when
///    the unsafe sits inside a *nested* loop within it: the hot-path
///    manually-unrolled-loop convention, where the invariant is proven once
///    at the outer loop and applies to every iteration of the unrolled inner
///    loop. The coverage is deliberately narrow — both the documented scope
///    and the unsafe's intervening scope must be loop bodies — so an opening
///    comment on a fn body, `if`/`match` arm, or bare block can't leak onto
///    an undocumented unsafe in an unrelated branch.
///
/// One such comment therefore suppresses every unsafe block in the covered
/// scope — the granularity is the enclosing scope, not the block.
///
/// We check the lines directly above the unsafe block, then walk up the
/// ancestor chain. For the nearest enclosing statement (`let_declaration` /
/// `expression_statement`) we check the lines above its start row: an inline
/// `unsafe` expression in the middle of a multi-line statement
/// (`let x = f(\n    unsafe { .. }\n);`) carries its safety comment above the
/// statement, not above the `unsafe` keyword's own row. Only the nearest
/// statement is consulted, and only when it starts on a different row than
/// the `unsafe` block, so a comment above an unrelated outer statement can't
/// leak. For an *outer* loop body reached only after crossing an inner loop
/// body we check whether a SAFETY comment sits at its start (between the
/// opening `{` and the first statement); the upward scan stops at the `{`'s
/// own row (real code), so it never leaks past the loop boundary. The
/// unsafe's own innermost loop body is *not* consulted this way — a comment
/// at the top of that loop documenting an earlier statement must not
/// blanket-cover a later undocumented sibling unsafe; that case is governed
/// by the direct-above scan. We then keep walking for enclosing `impl_item` /
/// `function_item` (the shared-invariant convention). Leakage from an
/// unrelated sibling item is prevented by the per-row scan stopping at the
/// first real-code line above each ancestor: a sibling's comment is never
/// directly above an ancestor's start row. The walk bound (`source_file`)
/// only caps how far up the chain we look.
fn has_safety_comment_above(node: tree_sitter::Node, source: &str) -> bool {
    let lines: Vec<&str> = source.lines().collect();
    if safety_comment_above_row(node.start_position().row, SkipLets::Yes, &lines) {
        return true;
    }
    let mut checked_enclosing_statement = false;
    let mut crossed_inner_loop_body = false;
    let mut cur = node.parent();
    while let Some(p) = cur {
        if !checked_enclosing_statement
            && matches!(p.kind(), "let_declaration" | "expression_statement")
        {
            checked_enclosing_statement = true;
            // The comment must sit directly above the statement: preceding
            // `let` bindings are independent statements whose own comments
            // would otherwise leak, so we do not skip them here.
            if p.start_position().row != node.start_position().row
                && safety_comment_above_row(p.start_position().row, SkipLets::No, &lines)
            {
                return true;
            }
        }
        if is_loop_body(p) {
            // Only an *outer* loop body reached after crossing an inner loop
            // body is treated as a block-level safety scope (the unrolled
            // inner-loop convention). The unsafe's own innermost loop body is
            // skipped so a top-of-loop comment documenting an earlier
            // statement can't blanket a later sibling unsafe.
            if crossed_inner_loop_body && block_opens_with_safety_comment(p, &lines) {
                return true;
            }
            crossed_inner_loop_body = true;
        }
        if matches!(p.kind(), "impl_item" | "function_item")
            && safety_comment_above_row(p.start_position().row, SkipLets::Yes, &lines)
        {
            return true;
        }
        cur = p.parent();
    }
    false
}

/// True if the FIRST named child inside the unsafe block's body is a
/// `// SAFETY:` comment (the inline-first convention). The safety
/// justification is placed as the opening line *inside* `unsafe { ... }`
/// rather than above the `unsafe` keyword; Clippy's
/// `undocumented_unsafe_blocks` — this rule's stated equivalent — accepts
/// that form. Only the first inner item is consulted: a SAFETY comment
/// buried after real statements documents that statement, not the block
/// opening, so it does not suppress the block.
fn has_safety_comment_inside_block(node: tree_sitter::Node, source: &str) -> bool {
    let mut cursor = node.walk();
    let Some(block) = node.children(&mut cursor).find(|c| c.kind() == "block") else {
        return false;
    };
    let mut block_cursor = block.walk();
    let Some(first) = block.named_children(&mut block_cursor).next() else {
        return false;
    };
    if !matches!(first.kind(), "line_comment" | "block_comment") {
        return false;
    }
    first
        .utf8_text(source.as_bytes())
        .is_ok_and(|text| is_safety_marker(text.trim_start()))
}

/// True if `node` is the `{ … }` body of a `while`/`loop`/`for` expression.
/// Block-level SAFETY coverage is restricted to loop bodies so that an
/// opening comment can't leak from a fn body, `if`/`match` arm, or bare block.
fn is_loop_body(node: tree_sitter::Node) -> bool {
    node.kind() == "block"
        && node.parent().is_some_and(|parent| {
            matches!(
                parent.kind(),
                "while_expression" | "loop_expression" | "for_expression"
            )
        })
}

/// True if `block` (a loop body) opens with a `// SAFETY:` comment: a
/// block-level rationale documenting the unsafe blocks nested within. We scan
/// upward from the block's first statement; the scan stops at the opening `{`
/// row (real code) so a comment belonging to the enclosing construct never
/// leaks in. The first statement must sit on a different row than the `{`
/// (otherwise there is no room for a leading comment, and `{ stmt }` carries
/// no block-level documentation).
fn block_opens_with_safety_comment(block: tree_sitter::Node, lines: &[&str]) -> bool {
    let mut cursor = block.walk();
    let Some(first_stmt) = block
        .named_children(&mut cursor)
        .find(|c| !matches!(c.kind(), "line_comment" | "block_comment"))
    else {
        return false;
    };
    if first_stmt.start_position().row == block.start_position().row {
        return false;
    }
    safety_comment_above_row(first_stmt.start_position().row, SkipLets::No, lines)
}

/// Whether `safety_comment_above_row` skips a contiguous run of preparatory
/// `let` bindings between the comment and the scanned row.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SkipLets {
    Yes,
    No,
}

/// True if a `// SAFETY:` comment sits on the lines directly above
/// `start_row`. We scan by text (the comment may be on any of the
/// preceding lines up to the previous code line) because tree-sitter
/// doesn't attach comments to expressions. Blank lines, other comments,
/// and outer attributes (`#[...]`) are skipped so a comment above an
/// `impl` carrying `#[allow(unsafe_code)]` still counts. When `skip_lets`
/// is `Yes`, a contiguous run of simple `let` bindings directly above is
/// also skipped: documenting the invariant once above the preparatory
/// bindings, then performing the unsafe call, is idiomatic. The scan stops
/// at the first non-`let` code line (a call/expression statement, an
/// opening/closing brace, another unsafe block), so a stray faraway comment
/// never counts.
fn safety_comment_above_row(start_row: usize, skip_lets: SkipLets, lines: &[&str]) -> bool {
    let mut row = start_row;
    while row > 0 {
        row -= 1;
        let Some(line) = lines.get(row) else { break };
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if trimmed.starts_with("//") || trimmed.starts_with("/*") {
            if is_safety_marker(trimmed) {
                return true;
            }
            continue;
        }
        if skip_lets == SkipLets::Yes && is_simple_let_binding(trimmed) {
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
    fn allows_rustdoc_level_two_safety_heading() {
        let source = "fn f(p: *const u8) {\n\
                      /// ## Safety\n\
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
    fn allows_lowercase_colon_safety_comment_issue_5261() {
        // Issue #5261: zune-image uses a lowercase `// safety:` marker.
        let source = "fn f(p: *const u8) {\n\
                      // safety: u8's can alias anything\n\
                      unsafe { let _ = *p; }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_lowercase_period_safety_comment_issue_5261() {
        // Issue #5261 (channel.rs:163): `// safety.` — lowercase + period.
        let source = "fn f(p: *const u8) {\n\
                      // safety.\n\
                      // all types can alias u8\n\
                      unsafe { let _ = *p; }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_unrelated_preceding_comment() {
        // A comment that doesn't carry a safety marker (even one that
        // mentions the word in prose) must not suppress the rule.
        let source = "fn f(p: *const u8) {\n\
                      // the safety of this depends on the caller\n\
                      unsafe { let _ = *p; }\n\
                      }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_comma_terminated_safety_comment_issue_7226() {
        // Issue #7226 (polars chunked_array/mod.rs): `// SAFETY,` — comma
        // terminator after a leading SAFETY marker.
        let source = "fn f(p: *const u8) {\n\
                      // SAFETY, we will not swap the PrimitiveArray.\n\
                      unsafe { let _ = *p; }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_dash_terminated_safety_comment_issue_7226() {
        // Issue #7226 (polars sort/arg_sort.rs): `//SAFETY -` — no space
        // after the sigil, dash terminator.
        let source = "fn f(p: *const u8) {\n\
                      //SAFETY - we allocated enough\n\
                      unsafe { let _ = *p; }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_space_terminated_safety_comment_issue_7226() {
        // Issue #7226: a bare `SAFETY` keyword followed by prose with no
        // punctuation terminator.
        let source = "fn f(p: *const u8) {\n\
                      // SAFETY we are within bounds\n\
                      unsafe { let _ = *p; }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_safety_embedded_in_longer_word_issue_7226() {
        // `safety` embedded in a longer word is not a leading SAFETY marker
        // (the boundary after `safety` must be non-alphanumeric).
        let source = "fn f(p: *const u8) {\n\
                      // safetycheck: run before deref\n\
                      unsafe { let _ = *p; }\n\
                      }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_safety_underscore_identifier_issue_7226() {
        // `safety_check` is a longer identifier, not a bare `safety` token:
        // `_` is part of the word, so it is not a leading SAFETY marker.
        let source = "fn f(p: *const u8) {\n\
                      // safety_check: run before deref\n\
                      unsafe { let _ = *p; }\n\
                      }";
        assert_eq!(run_on(source).len(), 1);
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

    #[test]
    fn allows_safety_comment_separated_by_cfg_attr_issue_5451() {
        // Issue #5451: jiff src/fmt/buffer.rs — a `// SAFETY:` comment
        // documents the unsafe block but a `#[cfg(...)]` attribute attached
        // to the enclosing `if let` sits between them. The unsafe block sits
        // on the `if let` line; the attribute is the preceding line. The
        // upward scan skips the attribute and finds the comment.
        let source = "fn f(wtr: &mut W, n: usize, with: usize) {\n\
                      // SAFETY: We only ever write valid UTF-8. Namely, `BorrowedBuffer`\n\
                      // enforces this invariant.\n\
                      #[cfg(feature = \"alloc\")]\n\
                      if let Some(buf) = unsafe { wtr.as_mut_vec() } {\n\
                      buf.reserve(n);\n\
                      }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_inline_unsafe_expr_in_multiline_let_issue_5465() {
        // Issue #5465: time-rs primitive_date_time.rs — a `// Safety:`
        // comment documents an inline `unsafe` expression that sits on its
        // own row inside a multi-line `let` statement (a function-call
        // argument). The comment is above the `let` statement, not above the
        // `unsafe` keyword's row, so the scan walks up to the enclosing
        // statement.
        let source = "fn f(buf: &mut [u8], date_len: usize) {\n\
                      // Safety: The buffer is large enough that the first chunk is in bounds.\n\
                      let time_len = self.time.fmt_into_buffer(\n\
                      unsafe { buf[date_len + 1..].first_chunk_mut().unwrap_unchecked() }\n\
                      );\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_inline_unsafe_expr_same_line_let_issue_5465() {
        // Issue #5465: time-rs num_fmt.rs — a multi-line `// Safety:` comment
        // above a `let` whose initializer is an inline `unsafe` expression on
        // the same row. The scan above the `unsafe` row skips the trailing
        // comment line and reaches the marker line.
        let source = "fn f(offset: usize, size: usize) {\n\
                      // Safety: `offset` is within the bounds of the array. The array\n\
                      // contains only ASCII characters, so it's valid UTF-8.\n\
                      let first_two = unsafe { str_from_raw_parts(ptr.add(offset), size) };\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_undocumented_inline_unsafe_expr_issue_5465() {
        // Negative: an inline `unsafe` expression inside a multi-line `let`
        // with no safety comment anywhere above the statement still fires.
        let source = "fn f(buf: &mut [u8], date_len: usize) {\n\
                      let time_len = self.time.fmt_into_buffer(\n\
                      unsafe { buf[date_len + 1..].first_chunk_mut().unwrap_unchecked() }\n\
                      );\n\
                      }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_inline_unsafe_expr_with_unrelated_comment_issue_5465() {
        // Negative: a comment that isn't a safety marker above the enclosing
        // statement must not suppress an inline `unsafe` expression.
        let source = "fn f(buf: &mut [u8], date_len: usize) {\n\
                      // compute the length of the formatted time component\n\
                      let time_len = self.time.fmt_into_buffer(\n\
                      unsafe { buf[date_len + 1..].first_chunk_mut().unwrap_unchecked() }\n\
                      );\n\
                      }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_nested_loop_unsafe_under_outer_block_safety_comment_issue_5973() {
        // Issue #5973: regex-automata src/dfa/search.rs — a `// SAFETY:`
        // comment at the start of the outer `while` block documents the
        // invariants for the manually-unrolled inner loop. All four unsafe
        // calls in the inner loop are covered by the one block-level comment.
        let source = "fn search(input: I) {\n\
                      while at < input.end() {\n\
                      // SAFETY: invariants we uphold in the loops below: 'sid' is\n\
                      // valid and 'at' is in bounds in the unrolled loop below.\n\
                      let mut prev_sid;\n\
                      while at < input.end() {\n\
                      prev_sid = unsafe { next(sid, at) };\n\
                      sid = unsafe { next(prev_sid, at) };\n\
                      prev_sid = unsafe { next(sid, at) };\n\
                      sid = unsafe { next(prev_sid, at) };\n\
                      }\n\
                      }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_nested_loop_unsafe_without_any_safety_comment_issue_5973() {
        // Negative: the same nested-loop shape but with NO SAFETY comment
        // anywhere in the ancestor block chain still fires once per unsafe.
        let source = "fn search(input: I) {\n\
                      while at < input.end() {\n\
                      let mut prev_sid;\n\
                      while at < input.end() {\n\
                      prev_sid = unsafe { next(sid, at) };\n\
                      sid = unsafe { next(prev_sid, at) };\n\
                      }\n\
                      }\n\
                      }";
        assert_eq!(run_on(source).len(), 2);
    }

    #[test]
    fn block_safety_comment_does_not_leak_to_sibling_unsafe_issue_5973() {
        // The block-level coverage applies to unsafe nested in a *sub-block*.
        // A comment at the top of a block documenting an earlier statement
        // must not blanket-cover a later *direct sibling* unsafe in the same
        // block separated by real code — that stays the direct-above scan's
        // job, which breaks on intervening code.
        let source = "fn f(p: *const u8) {\n\
                      // SAFETY: documents the call below, not the unsafe read.\n\
                      do_setup();\n\
                      unsafe { let _ = *p; }\n\
                      }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn block_safety_comment_does_not_leak_to_if_arm_unsafe_issue_5973() {
        // Block-level coverage is restricted to nested loop bodies. An opening
        // SAFETY comment on a fn body that documents an earlier statement must
        // not leak onto an undocumented unsafe inside an `if` arm.
        let source = "fn f(p: *const u8, cond: bool) {\n\
                      // SAFETY: documents the helper call below.\n\
                      helper();\n\
                      if cond {\n\
                      let _ = unsafe { *p };\n\
                      }\n\
                      }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn block_safety_comment_does_not_leak_into_inner_loop_from_fn_body_issue_5973() {
        // The documented scope must itself be a loop body. An opening SAFETY
        // comment on the fn body (documenting an earlier statement) must not
        // cover an undocumented unsafe inside an inner loop.
        let source = "fn f(p: *const u8) {\n\
                      // SAFETY: documents the helper call below.\n\
                      helper();\n\
                      while cond {\n\
                      let _ = unsafe { *p };\n\
                      }\n\
                      }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn nested_block_own_safety_comment_takes_precedence_issue_5973() {
        // A nested sub-block whose unsafe carries its own SAFETY comment is
        // covered by that comment (direct-above), independent of any
        // outer-block comment — per-unsafe documentation is not regressed.
        let source = "fn search(input: I) {\n\
                      while at < input.end() {\n\
                      while at < input.end() {\n\
                      // SAFETY: 'sid' is valid and 'at' is in bounds.\n\
                      sid = unsafe { next(sid, at) };\n\
                      }\n\
                      }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_inline_unsafe_expr_when_comment_belongs_to_prior_let_issue_5465() {
        // Negative: a `// SAFETY:` comment that documents a *prior* `let`
        // statement must not leak onto an undocumented inline unsafe
        // expression in the next statement. The enclosing-statement scan must
        // not skip the intervening `let` (it belongs to the comment, not to
        // the unsafe block).
        let source = "fn f(ptr: *const u8) {\n\
                      // SAFETY: documents the call below, not the unsafe read.\n\
                      let a = compute();\n\
                      let b = other(\n\
                      unsafe { ptr.read() }\n\
                      );\n\
                      }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_inline_first_safety_comment_issue_6265() {
        // Issue #6265: tokio-rs/bytes src/bytes_mut.rs — the `// SAFETY:`
        // comment is the first line *inside* the unsafe block body
        // (inline-first convention, accepted by Clippy's
        // `undocumented_unsafe_blocks`), not above the `unsafe` keyword.
        let source = "fn f(at: usize) -> Self {\n\
                      unsafe {\n\
                      // SAFETY: `shallow_clone` increments the reference count\n\
                      // and returns a bitwise copy of the handle.\n\
                      let mut other = self.shallow_clone();\n\
                      // SAFETY: We've checked that `at` <= `self.capacity()`.\n\
                      other.advance_unchecked(at);\n\
                      other\n\
                      }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_inline_first_block_comment_safety_issue_6265() {
        // A `/* SAFETY: ... */` block comment as the first inner line also
        // documents the block (the `block_comment` arm).
        let source = "fn f(p: *const u8) {\n\
                      unsafe {\n\
                      /* SAFETY: p is non-null and valid. */\n\
                      let _ = *p;\n\
                      }\n\
                      }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_inline_first_non_safety_first_line_issue_6265() {
        // Negative control: an unsafe block with no SAFETY comment above and
        // whose first inner line is a plain statement still fires.
        let source = "fn f(p: *const u8) {\n\
                      unsafe {\n\
                      let x = compute();\n\
                      let _ = *p;\n\
                      }\n\
                      }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_inline_non_safety_first_comment_issue_6265() {
        // Negative control: a non-SAFETY comment as the first inner line does
        // not document the block.
        let source = "fn f(p: *const u8) {\n\
                      unsafe {\n\
                      // increment the counter first\n\
                      let _ = *p;\n\
                      }\n\
                      }";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_inline_safety_comment_not_first_issue_6265() {
        // Negative control: a `// SAFETY:` comment buried after a real
        // statement documents that statement, not the block opening — the
        // block stays flagged.
        let source = "fn f(p: *const u8) {\n\
                      unsafe {\n\
                      let x = compute();\n\
                      // SAFETY: x is in bounds.\n\
                      let _ = *p;\n\
                      }\n\
                      }";
        assert_eq!(run_on(source).len(), 1);
    }
}
