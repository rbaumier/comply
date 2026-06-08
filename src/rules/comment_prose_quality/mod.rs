//! comment-prose-quality

mod oxc_typescript;
mod rust;
mod text;

use crate::diagnostic::{Diagnostic, Severity};
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::{Backend, CheckCtx};
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "comment-prose-quality",
    description: "Comments with weasel words, passive voice, or lexical illusions \
                  reduce clarity.",
    remediation: "Rewrite the comment to be direct. Replace passive voice with \
                  active. Remove filler words. Fix repeated words.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["comments"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

const WEASEL_WORDS: &[&str] = &[
    "various",
    "many",
    "somewhat",
    "practically",
    "relatively",
    "extremely",
    "basically",
    "actually",
    "really",
    "literally",
    "quite",
    "fairly",
];

const PASSIVE_PATTERNS: &[&str] = &[
    "is used",
    "was called",
    "are handled",
    "were created",
    "been processed",
];

/// Strip the leading comment marker(s) from a single source line. Mirrors
/// the original `text.rs` behaviour — including stripping Rust doc-comment
/// markers (`//!`, `///`) so they don't trigger lexical-illusion on `!`/`/`.
fn strip_marker(line: &str) -> &str {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("//") {
        let rest = rest.strip_prefix('!').unwrap_or(rest);
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        return rest;
    }
    if let Some(rest) = trimmed.strip_prefix("/*") {
        let rest = rest.strip_suffix("*/").unwrap_or(rest);
        return rest;
    }
    if let Some(rest) = trimmed.strip_prefix('*') {
        return rest;
    }
    trimmed
}

/// True for a `comply-ignore` / `comply-ignore-file` directive line (after its
/// comment marker is stripped). Those lines reference rule identifiers and
/// carry meta-explanations, not the file's prose — the linter skips them, so a
/// weasel word *inside a rule id* (`too-many-break-or-continue`) never fires.
fn is_comply_directive(stripped_trimmed: &str) -> bool {
    stripped_trimmed.starts_with("comply-ignore")
}

/// Word-boundary, case-insensitive substring match.
fn contains_word(haystack: &str, needle: &str) -> bool {
    let lower = haystack.to_lowercase();
    let mut start = 0;
    while let Some(idx) = lower[start..].find(needle) {
        let abs = start + idx;
        let before_ok = abs == 0 || !lower.as_bytes()[abs - 1].is_ascii_alphanumeric();
        let after_pos = abs + needle.len();
        let after_ok =
            after_pos >= lower.len() || !lower.as_bytes()[after_pos].is_ascii_alphanumeric();
        if before_ok && after_ok {
            return true;
        }
        start = abs + 1;
    }
    false
}

/// Lint a set of comment AST nodes. Walks each comment line by line so that
/// the lexical-illusion check (last word of line N == first word of line N+1)
/// behaves exactly like the original text-based scan. Across nodes the
/// "previous word" only carries over when nodes are on adjacent source
/// lines — otherwise unrelated comments would falsely trigger.
pub(crate) fn lint_comment_nodes(
    ctx: &CheckCtx,
    source: &[u8],
    nodes: &[tree_sitter::Node<'_>],
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut prev_last_word: Option<(String, usize)> = None;
    let mut prev_line: Option<usize> = None;
    let mut text_of_prev_line: Option<String> = None;

    for node in nodes {
        let Ok(raw) = node.utf8_text(source) else {
            continue;
        };
        let is_doc_comment = raw.starts_with("///")
            || raw.starts_with("//!")
            || raw.starts_with("/**");
        let start_row = node.start_position().row;
        let mut in_code_block = false;
        for (offset, line) in raw.lines().enumerate() {
            let line_no = start_row + offset + 1;
            let text = strip_marker(line);
            let trimmed = text.trim();

            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block || (is_doc_comment && trimmed.starts_with("    ")) {
                continue;
            }
            if is_comply_directive(trimmed) {
                prev_last_word = None;
                prev_line = None;
                text_of_prev_line = None;
                continue;
            }

            let lower = text.to_lowercase();

            if !is_doc_comment {
                // Weasel words — only in inline comments, not doc comments.
                for &weasel in WEASEL_WORDS {
                    if contains_word(&lower, weasel) {
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: line_no,
                            column: 1,
                            rule_id: META.id.into(),
                            message: format!("Weasel word `{weasel}` in comment — be specific."),
                            severity: Severity::Warning,
                            span: None,
                        });
                        break;
                    }
                }

                // Passive voice — only in inline comments.
                for &passive in PASSIVE_PATTERNS {
                    if lower.contains(passive) {
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: line_no,
                            column: 1,
                            rule_id: META.id.into(),
                            message: format!(
                                "Passive voice `{passive}` in comment — use active voice."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                        break;
                    }
                }
            }

            // Lexical illusion: last word of previous comment line == first
            // word of this line. Only triggers when the previous line is
            // immediately adjacent (line_no - 1).
            let words: Vec<&str> = text.split_whitespace().collect();
            let is_heading_echo = prev_last_word.as_ref().is_some_and(|(_, wc)| {
                *wc == 2
                    && text_of_prev_line
                        .as_deref()
                        .is_some_and(|pt| pt.trim().starts_with("# "))
            });
            if let Some((ref prev, prev_wc)) = prev_last_word
                && let Some(prev_l) = prev_line
                && prev_l + 1 == line_no
                && words.len() > 1
                && prev_wc > 1
                && let Some(&first) = words.first()
                && first.chars().any(|c| c.is_alphabetic())
                && first.to_lowercase() == *prev
                && !is_heading_echo
            {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: line_no,
                    column: 1,
                    rule_id: META.id.into(),
                    message: format!("Lexical illusion: `{first}` repeated across lines."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            prev_last_word = words
                .last()
                .filter(|w| w.chars().any(|c| c.is_alphabetic()))
                .map(|w| (w.to_lowercase(), words.len()));
            prev_line = Some(line_no);
            text_of_prev_line = Some(text.to_string());
        }
    }
    diagnostics
}

/// Lint a set of comment spans given as `(raw_text, start_row_0based)` pairs.
/// Shared between the tree-sitter and oxc backends.
pub(crate) fn lint_comment_spans(
    ctx: &CheckCtx,
    spans: &[(&str, usize)],
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let mut prev_last_word: Option<(String, usize)> = None;
    let mut prev_line: Option<usize> = None;
    let mut text_of_prev_line: Option<String> = None;

    for &(raw, start_row) in spans {
        let is_doc_comment = raw.starts_with("///")
            || raw.starts_with("//!")
            || raw.starts_with("/**");
        let mut in_code_block = false;
        for (offset, line) in raw.lines().enumerate() {
            let line_no = start_row + offset + 1;
            let text = strip_marker(line);
            let trimmed = text.trim();

            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }
            if in_code_block || (is_doc_comment && trimmed.starts_with("    ")) {
                continue;
            }
            if is_comply_directive(trimmed) {
                prev_last_word = None;
                prev_line = None;
                text_of_prev_line = None;
                continue;
            }

            let lower = text.to_lowercase();

            if !is_doc_comment {
                for &weasel in WEASEL_WORDS {
                    if contains_word(&lower, weasel) {
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: line_no,
                            column: 1,
                            rule_id: META.id.into(),
                            message: format!("Weasel word `{weasel}` in comment — be specific."),
                            severity: Severity::Warning,
                            span: None,
                        });
                        break;
                    }
                }
                for &passive in PASSIVE_PATTERNS {
                    if lower.contains(passive) {
                        diagnostics.push(Diagnostic {
                            path: std::sync::Arc::clone(&ctx.path_arc),
                            line: line_no,
                            column: 1,
                            rule_id: META.id.into(),
                            message: format!(
                                "Passive voice `{passive}` in comment — use active voice."
                            ),
                            severity: Severity::Warning,
                            span: None,
                        });
                        break;
                    }
                }
            }

            let words: Vec<&str> = text.split_whitespace().collect();
            let is_heading_echo = prev_last_word.as_ref().is_some_and(|(_, wc)| {
                *wc == 2
                    && text_of_prev_line
                        .as_deref()
                        .is_some_and(|pt| pt.trim().starts_with("# "))
            });
            if let Some((ref prev, prev_wc)) = prev_last_word
                && let Some(prev_l) = prev_line
                && prev_l + 1 == line_no
                && words.len() > 1
                && prev_wc > 1
                && let Some(&first) = words.first()
                && first.chars().any(|c| c.is_alphabetic())
                && first.to_lowercase() == *prev
                && !is_heading_echo
            {
                diagnostics.push(Diagnostic {
                    path: std::sync::Arc::clone(&ctx.path_arc),
                    line: line_no,
                    column: 1,
                    rule_id: META.id.into(),
                    message: format!("Lexical illusion: `{first}` repeated across lines."),
                    severity: Severity::Warning,
                    span: None,
                });
            }
            prev_last_word = words
                .last()
                .filter(|w| w.chars().any(|c| c.is_alphabetic()))
                .map(|w| (w.to_lowercase(), words.len()));
            prev_line = Some(line_no);
            text_of_prev_line = Some(text.to_string());
        }
    }
    diagnostics
}

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::Tsx,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Rust, Backend::TreeSitter(Box::new(rust::Check))),
            (Language::Vue, Backend::Text(Box::new(text::Check))),
        ],
    }
}
