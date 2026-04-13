//! comment-paraphrases-code Rust backend.
//!
//! Flags comments that restate the function name in Rust source.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::walker::walk_tree;

const MAX_COMMENT_TOKENS: usize = 6;
const OVERLAP_THRESHOLD: f32 = 0.80;

const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "to", "of", "in", "on", "for", "with", "and", "or", "but", "is", "it",
    "this", "that", "these", "those",
];

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let source = ctx.source.as_bytes();
        let mut diagnostics = Vec::new();
        walk_tree(tree, |node| {
            let name_node = match node.kind() {
                "function_item" | "function_signature_item" => {
                    node.child_by_field_name("name")
                }
                _ => return,
            };
            let Some(name_node) = name_node else { return };
            let Ok(name) = name_node.utf8_text(source) else { return };

            // Find preceding comment sibling.
            let Some(prev) = node.prev_named_sibling() else { return };
            if prev.kind() != "line_comment" && prev.kind() != "block_comment" {
                return;
            }
            let Ok(comment_text) = prev.utf8_text(source) else { return };
            // Skip doc comments (/// or //!).
            if comment_text.starts_with("///") || comment_text.starts_with("//!") {
                return;
            }
            let body = strip_comment_markers(comment_text);
            if !looks_like_paraphrase(name, &body) {
                return;
            }
            let pos = prev.start_position();
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
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
        });
        diagnostics
    }
}

fn strip_comment_markers(text: &str) -> String {
    text.trim_start_matches("//")
        .trim_start_matches("/*")
        .trim_end_matches("*/")
        .trim()
        .to_string()
}

fn looks_like_paraphrase(identifier: &str, comment_body: &str) -> bool {
    let id_tokens = tokenize_identifier(identifier);
    if id_tokens.is_empty() {
        return false;
    }
    let comment_tokens = tokenize_comment(comment_body);
    if comment_tokens.is_empty() || comment_tokens.len() > MAX_COMMENT_TOKENS {
        return false;
    }
    let overlap = comment_tokens
        .iter()
        .filter(|token| id_tokens.contains(token))
        .count();
    // comply-ignore: rust-no-lossy-as-cast — bounded count.
    let ratio = overlap as f32 / comment_tokens.len() as f32;
    ratio >= OVERLAP_THRESHOLD
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
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rust(source, &Check)
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
