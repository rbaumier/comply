//! comment-requires-tag — an inline comment opens with a tag or does not exist.
//!
//! The name and the types carry the *what*.
//! A tag declares what a comment adds on top.
//! It opens the block, so a reader can skip from the first word.

mod oxc_typescript;
mod rust;
mod vue;

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::{Backend, CheckCtx};
use crate::rules::comment_blocks::{self, CommentBlock, RawComment};
use crate::rules::meta::RuleMeta;
use std::sync::Arc;

pub const META: RuleMeta = RuleMeta {
    id: "comment-requires-tag",
    description: "Comment opens with no tag, so nothing marks it as worth reading.",
    remediation: "Delete the comment — the name and the types already carry the what. \
                  Keep it only by opening the block with a tag: `why:` for a decision the \
                  code cannot show, `gotcha:` for a trap, `TODO(#123):` / `FIXME(#123):` / \
                  `WORKAROUND(upstream#123):` / `HACK(#123):` for an action with its \
                  reference, or `SAFETY:` for an unsafe block's invariant.",
    severity: Severity::Error,
    doc_url: None,
    categories: &["comments"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (Language::TypeScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::JavaScript, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::TreeSitter(Box::new(vue::Check))),
        ],
    }
}

const MESSAGE: &str = "Untagged comment — delete it, or open the block with \
                       `why:`, `gotcha:`, or `TODO(#123):`.";

/// Tags that license a note to a reader, in their `<tag>:` form.
/// Matched case-insensitively: authors write both `why:` and `Why:`.
const PROSE_TAGS: &[&str] = &["why:", "gotcha:"];

/// Tags that license a note about pending work.
/// Each names an action, so it counts only with the reference tracking it.
/// `TODO(#123)`, never a bare `TODO`.
const ACTION_TAGS: &[&str] = &["todo", "fixme", "workaround", "hack"];

/// Turn every comment of one file into diagnostics.
///
/// Shared by all four backends.
/// They differ only in how they read comment tokens out of their language.
pub(crate) fn diagnose(comments: Vec<RawComment>, ctx: &CheckCtx) -> Vec<Diagnostic> {
    let in_scope = comments.into_iter().filter(is_in_scope).collect();
    comment_blocks::merge(in_scope)
        .into_iter()
        .filter(needs_tag)
        .map(|block| Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line: block.line,
            column: block.column,
            rule_id: META.id.into(),
            message: MESSAGE.into(),
            severity: Severity::Error,
            span: None,
        })
        .collect()
}

/// True for a comment token this rule may judge.
///
/// A doc comment states a contract; a tool directive is machine input.
/// Dropping them before the merge also splits the run a directive sits in.
/// The prose below then answers on its own opening line.
fn is_in_scope(comment: &RawComment) -> bool {
    let first_line = comment.raw.lines().next().unwrap_or("");
    !comment_blocks::is_doc_comment(&comment.raw)
        && !comment_blocks::is_tool_directive(&comment_blocks::strip_markers(first_line))
}

/// True when a block owes the reader a tag and has none.
fn needs_tag(block: &CommentBlock) -> bool {
    let Some(opening) = block.lines.iter().find(|line| !line.text.is_empty()) else {
        // Nothing but markers: an empty `//` or `/* */` says nothing to tag.
        return false;
    };
    !is_ruling(&opening.text)
        && !opens_with_tag(&opening.text)
        && !block.is_license()
        && !is_commented_out_code(block)
}

/// True when the block's opening line leads with one of the accepted tags.
fn opens_with_tag(opening: &str) -> bool {
    let lower = opening.trim_start().to_ascii_lowercase();
    PROSE_TAGS.iter().any(|tag| lower.starts_with(tag))
        || crate::rules::rust_helpers::is_safety_marker(opening)
        || ACTION_TAGS.iter().any(|tag| carries_reference(&lower, tag))
}

/// True when `lower` opens with `tag` and a non-empty parenthesized reference.
/// `todo(#123)`, `workaround(upstream#704)`.
/// Whether the reference is *usable* is `todo-needs-issue-link`'s question.
fn carries_reference(lower: &str, tag: &str) -> bool {
    let Some(after_tag) = lower.strip_prefix(tag) else {
        return false;
    };
    let Some(inside) = after_tag.strip_prefix('(') else {
        return false;
    };
    inside
        .find(')')
        .is_some_and(|close| !inside[..close].trim().is_empty())
}

/// Copies of one ruling character in a row before a line reads as drawn.
/// Three is what `// --- Helpers ---` uses.
const RULING_RUN: usize = 3;

/// True for a line drawn rather than written.
/// A decorative ruling (`// ----`, `// ── Helpers ──`), or a letterless line.
///
/// Read on the opening line only, which exempts a boxed banner's title too.
/// A heading is structure, not a note to the reader.
fn is_ruling(text: &str) -> bool {
    !text.chars().any(char::is_alphanumeric) || has_ruling_run(text)
}

/// True when `text` holds `RULING_RUN` copies of one ruling character in a row.
/// The set holds only the characters used to draw a line.
/// An ellipsis (`...`) or an emphatic `!!!` inside prose does not qualify.
fn has_ruling_run(text: &str) -> bool {
    let is_ruling_char =
        |c: char| matches!(c, '-' | '=' | '_' | '~' | '*' | '#' | '+' | '\u{2500}'..='\u{257F}');
    let mut run = 0;
    let mut previous = None;
    for character in text.chars() {
        if !is_ruling_char(character) {
            run = 0;
            previous = None;
            continue;
        }
        run = if previous == Some(character) { run + 1 } else { 1 };
        previous = Some(character);
        if run >= RULING_RUN {
            return true;
        }
    }
    false
}

/// True when the block may be commented-out code.
/// `no-commented-out-code` already reports that, with the same remedy.
/// Reuses its own pre-parse filter, so neither rule can widen past the other.
/// Prose carrying a `;` or a `{` is exempted too: one finding beats two.
fn is_commented_out_code(block: &CommentBlock) -> bool {
    crate::rules::no_commented_out_code::has_code_shape(&block.prose())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(raw: &str) -> RawComment {
        RawComment { start_byte: 0, line: 1, column: 1, raw: raw.into(), is_line: true }
    }

    fn flags(raw: &str) -> bool {
        let in_scope: Vec<RawComment> = vec![line(raw)].into_iter().filter(is_in_scope).collect();
        comment_blocks::merge(in_scope).iter().any(needs_tag)
    }

    #[test]
    fn prose_tags_are_case_insensitive() {
        assert!(!flags("// why: the upstream 404 means the version was purged"));
        assert!(!flags("// Why: the upstream 404 means the version was purged"));
        assert!(!flags("// GOTCHA: getSession returns null right after a refresh"));
    }

    #[test]
    fn action_tag_needs_its_reference() {
        assert!(!flags("// TODO(#123): migrate to v2"));
        assert!(!flags("// WORKAROUND(upstream#704): the fix is unreleased"));
        assert!(flags("// TODO: migrate to v2"));
        assert!(flags("// FIXME - broken on the edge case"));
        assert!(flags("// HACK() no reference at all"));
    }

    #[test]
    fn tag_must_open_the_block() {
        assert!(flags("// this explains the retry loop, why: the broker drops frames"));
    }

    #[test]
    fn safety_marker_counts_as_a_tag() {
        assert!(!flags("// SAFETY: the pointer comes from a live Box"));
        assert!(flags("// safetycheck runs before the cast"));
    }

    #[test]
    fn rulings_and_empty_comments_are_not_prose() {
        assert!(!flags("// ----------------"));
        assert!(!flags("// ── Helpers ──────"));
        assert!(!flags("// --- helpers at the crate root ---"));
        assert!(!flags("//"));
        assert!(!flags("// ‘ … ’"));
        // A two-character dash run is an argument name, not a ruling.
        assert!(flags("// the parser skips a --flag argument"));
        // An ellipsis and an emphatic run are prose, not drawing.
        assert!(flags("// the server grows the payload additively... so stay non-strict"));
        assert!(flags("// never reorder these two calls!!!"));
    }

    #[test]
    fn a_boxed_banner_exempts_its_title() {
        let banner = vec![
            line("// ═══════════════════"),
            RawComment { start_byte: 24, line: 2, ..line("// ASYNC / PROMISES") },
            RawComment { start_byte: 48, line: 3, ..line("// ═══════════════════") },
        ];
        let in_scope = banner.into_iter().filter(is_in_scope).collect();
        assert!(!comment_blocks::merge(in_scope).iter().any(needs_tag));
    }

    #[test]
    fn license_banner_is_exempt() {
        assert!(!flags("// Copyright 2026 the authors, all rights reserved"));
    }

    #[test]
    fn code_shaped_comment_is_left_to_no_commented_out_code() {
        assert!(!flags("// const x = compute();"));
    }
}
