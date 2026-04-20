//! comment-paraphrases-code — Vue text backend.
//!
//! Scans Vue SFC `<script>` sections for comments that paraphrase function names.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use crate::rules::vue_template_helpers::is_vue_file;

const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "to", "of", "in", "on", "for", "with", "and", "or", "but", "is", "it",
    "this", "that", "these", "those",
];

#[derive(Debug)]
pub struct Check;

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_vue_file(ctx.path) {
            return Vec::new();
        }
        let max_comment_tokens = ctx
            .config
            .threshold("comment-paraphrases-code", "max_comment_tokens");
        let overlap_threshold = ctx
            .config
            .float("comment-paraphrases-code", "overlap_threshold") as f32;
        let mut diagnostics = Vec::new();
        let lines: Vec<&str> = ctx.source.lines().collect();

        for (i, line) in lines.iter().enumerate() {
            let trimmed = line.trim();
            // Extract function name from function declarations.
            let fn_name = extract_fn_name(trimmed);
            let Some(fn_name) = fn_name else { continue };

            // Check preceding line for a comment.
            if i == 0 {
                continue;
            }
            let prev = lines[i - 1].trim();
            let comment_body = if let Some(rest) = prev.strip_prefix("//") {
                // Skip JSDoc-style.
                if rest.starts_with('/') || rest.starts_with('!') {
                    continue;
                }
                rest.trim()
            } else {
                continue;
            };

            if looks_like_paraphrase(fn_name, comment_body, max_comment_tokens, overlap_threshold) {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: i, // comment line (1-based: i is 0-based for prev line)
                    column: 1,
                    rule_id: "comment-paraphrases-code".into(),
                    message: format!(
                        "Comment above `{fn_name}` paraphrases the function name."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

fn extract_fn_name(line: &str) -> Option<&str> {
    // Match patterns like `function foo(`, `async function foo(`, `const foo = (`
    let rest = line
        .strip_prefix("export ")
        .unwrap_or(line);
    let rest = rest
        .strip_prefix("async ")
        .unwrap_or(rest);
    if let Some(after) = rest.strip_prefix("function ") {
        let name_end = after.find(|c: char| !c.is_alphanumeric() && c != '_')?;
        if name_end > 0 {
            return Some(&after[..name_end]);
        }
    }
    None
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
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("component.vue"), source))
    }

    #[test]
    fn flags_paraphrase() {
        let src = "<script>\n// get user\nfunction getUser() {}\n</script>";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_meaningful() {
        let src = "<script>\n// Hits DB with JOIN to avoid N+1\nfunction getUser() {}\n</script>";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_vue() {
        let diags = Check.check(&CheckCtx::for_test(
            Path::new("file.ts"),
            "// get user\nfunction getUser() {}",
        ));
        assert!(diags.is_empty());
    }
}
