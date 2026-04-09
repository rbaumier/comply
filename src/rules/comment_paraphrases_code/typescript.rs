//! comment-paraphrases-code backend — short comments that share most of
//! their tokens with the documented identifier.
//!
//! Algorithm:
//! 1. Walk every `function_declaration`, `method_definition`, and
//!    arrow-function-bound `variable_declarator`.
//! 2. Find the immediately preceding sibling comment (single-line `//` or
//!    block `/* */`). Skip JSDoc (`/** */`) — that's documentation, not a
//!    paraphrase candidate, and `jsdoc-on-exported`/`jsdoc-missing-example`
//!    handle the doc case.
//! 3. Tokenize both the function name (camelCase/snake_case → words) and
//!    the comment body (split on whitespace, lowercase, drop stop-words).
//! 4. If the comment is short (≤ 6 tokens) AND ≥ 80% of its non-stop-word
//!    tokens overlap the function name's tokens, flag it.
//!
//! The 80% threshold is intentionally conservative — we only fire when the
//! comment is essentially a restatement. Long comments that include the
//! function name plus actual context are not flagged.

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
                "function_declaration" | "method_definition" => {
                    node.child_by_field_name("name")
                }
                "variable_declarator" => {
                    let value = node.child_by_field_name("value");
                    if !matches!(
                        value.map(|v| v.kind()),
                        Some("arrow_function") | Some("function_expression")
                    ) {
                        return;
                    }
                    node.child_by_field_name("name")
                }
                _ => return,
            };
            let Some(name_node) = name_node else { return };
            let Ok(name) = name_node.utf8_text(source) else { return };
            // The comment must precede the documented item. For function
            // declarations the relevant sibling is the function itself; for
            // arrow-function variable_declarators the comment lives above
            // the wrapping `lexical_declaration`, so walk up until we find
            // a node that has a sibling.
            let anchor = if node.kind() == "variable_declarator" {
                node.parent().unwrap_or(node)
            } else {
                node
            };
            let Some(prev) = anchor.prev_named_sibling() else { return };
            if prev.kind() != "comment" {
                return;
            }
            let Ok(comment_text) = prev.utf8_text(source) else { return };
            // Skip JSDoc — that's a different rule's responsibility.
            if comment_text.starts_with("/**") {
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
                     explain WHY (what breaks if this is deleted?), not WHAT — or delete \
                     the comment if no consequence comes to mind."
                ),
                severity: Severity::Warning,
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

/// True if the comment looks like a paraphrase of the identifier.
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
    let ratio = overlap as f32 / comment_tokens.len() as f32;
    ratio >= OVERLAP_THRESHOLD
}

/// Split a camelCase or snake_case identifier into lowercase word tokens.
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

/// Split a comment body into lowercase word tokens, dropping stop-words.
fn tokenize_comment(body: &str) -> Vec<String> {
    body.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_ascii_lowercase())
        .filter(|w| !STOP_WORDS.contains(&w.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        Check.check(&CheckCtx::for_test(Path::new("t.ts"), source), &tree)
    }

    #[test]
    fn flags_paraphrase() {
        // "handle click" tokens [handle, click] all in identifier
        // [handle, click], 100% overlap → flag.
        let source = "// handle click\nfunction handleClick() {}";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn flags_snake_case_paraphrase() {
        let source = "// fetch user data\nconst fetch_user_data = () => {};";
        assert_eq!(run_on(source).len(), 1);
    }

    #[test]
    fn allows_informative_comment() {
        // Comment explains WHY, vocabulary diverges → no flag.
        let source = "// Avoid double-fire when the user double-clicks fast\nfunction handleClick() {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_jsdoc() {
        // JSDoc is handled by a different rule.
        let source = "/** Handle click */\nfunction handleClick() {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn ignores_long_comments() {
        // > 6 tokens → not a paraphrase candidate.
        let source = "// handle click event by deduping fast double click bursts and recording metrics\nfunction handleClick() {}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn unit_tokenize_identifier() {
        assert_eq!(tokenize_identifier("handleClick"), vec!["handle", "click"]);
        assert_eq!(tokenize_identifier("fetch_user_data"), vec!["fetch", "user", "data"]);
        assert_eq!(tokenize_identifier("HTTPClient"), vec!["h", "t", "t", "p", "client"]);
    }

    #[test]
    fn unit_tokenize_comment() {
        assert_eq!(tokenize_comment("the handle click"), vec!["handle", "click"]);
        assert_eq!(tokenize_comment("a fetch and a parse"), vec!["fetch", "parse"]);
    }
}
