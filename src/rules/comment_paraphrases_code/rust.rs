//! comment-paraphrases-code Rust backend.
//!
//! Flags comments that restate the function name in Rust source.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "to", "of", "in", "on", "for", "with", "and", "or", "but", "is", "it",
    "this", "that", "these", "those",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["function_item", "function_signature_item"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let max_comment_tokens = ctx
            .config
            .threshold("comment-paraphrases-code", "max_comment_tokens", ctx.lang);
        let overlap_threshold =
            ctx.config
                .float("comment-paraphrases-code", "overlap_threshold", ctx.lang) as f32;
        let source = ctx.source.as_bytes();
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let Ok(name) = name_node.utf8_text(source) else {
            return;
        };

        // Find preceding comment sibling.
        let Some(prev) = node.prev_named_sibling() else {
            return;
        };
        if prev.kind() != "line_comment" && prev.kind() != "block_comment" {
            return;
        }
        let Ok(comment_text) = prev.utf8_text(source) else {
            return;
        };
        // Skip doc comments (/// or //!).
        if comment_text.starts_with("///") || comment_text.starts_with("//!") {
            return;
        }
        let body = strip_comment_markers(comment_text);
        if !looks_like_paraphrase(name, &body, max_comment_tokens, overlap_threshold) {
            return;
        }
        let pos = prev.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "comment-paraphrases-code".into(),
            message: format!(
                "Comment above `{name}` paraphrases the function name. Rewrite to \
                 explain WHY, not WHAT — or delete the comment."
            ),
            severity: Severity::Warning,
            span: None,
        });
    }
}

fn strip_comment_markers(text: &str) -> String {
    text.trim_start_matches("//")
        .trim_start_matches("/*")
        .trim_end_matches("*/")
        .trim()
        .to_string()
}

fn looks_like_paraphrase(
    identifier: &str,
    comment_body: &str,
    max_comment_tokens: usize,
    overlap_threshold: f32,
) -> bool {
    let id_tokens = tokenize_identifier(identifier);
    if id_tokens.is_empty() {
        return false;
    }
    let comment_tokens = tokenize_comment(comment_body);
    if comment_tokens.is_empty() || comment_tokens.len() > max_comment_tokens {
        return false;
    }
    let overlap = comment_tokens
        .iter()
        .filter(|token| id_tokens.contains(token))
        .count();
    // comply-ignore: rust-no-lossy-as-cast — bounded count.
    let ratio = overlap as f32 / comment_tokens.len() as f32;
    ratio >= overlap_threshold
}

fn tokenize_identifier(name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for ch in name.chars() {
        if ch == '_' || ch == '-' {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            continue;
        }
        if ch.is_ascii_uppercase() && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        current.push(ch.to_ascii_lowercase());
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

fn tokenize_comment(body: &str) -> Vec<String> {
    body.split_whitespace()
        .map(|w| w.to_ascii_lowercase())
        .filter(|w| !STOP_WORDS.contains(&w.as_str()))
        .collect()
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
    fn flags_paraphrase_comment() {
        let src = "// get user\nfn get_user() {}\n";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_meaningful_comment() {
        let src = "// Hits the DB with a JOIN to avoid N+1\nfn get_user() {}\n";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn skips_doc_comments() {
        let src = "/// get user\nfn get_user() {}\n";
        assert!(run_on(src).is_empty());
    }
}
