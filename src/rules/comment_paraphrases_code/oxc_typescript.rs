//! comment-paraphrases-code OXC backend.
//!
//! Scans comments + function/method/variable declarations to detect
//! comments that merely restate the function name.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "to", "of", "in", "on", "for", "with", "and", "or", "but", "is", "it",
    "this", "that", "these", "those",
];

impl OxcCheck for Check {
    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let max_comment_tokens = ctx
            .config
            .threshold("comment-paraphrases-code", "max_comment_tokens", ctx.lang);
        let overlap_threshold =
            ctx.config
                .float("comment-paraphrases-code", "overlap_threshold", ctx.lang) as f32;

        let source = ctx.source;
        let comments = semantic.comments();
        let nodes = semantic.nodes();
        let mut diagnostics = Vec::new();

        // Build a sorted list of comment positions for quick lookup.
        // For each function/method/variable declarator, find the preceding comment.
        // We iterate all nodes and check for function-like nodes.
        for node in nodes.iter() {
            let (name, anchor_start) = match node.kind() {
                oxc_ast::AstKind::Function(func) => {
                    let name = func.id.as_ref().map(|id| id.name.as_str());
                    // For methods, the name comes from the parent MethodDefinition
                    if name.is_none() {
                        // Try parent for method_definition
                        let parent_id = nodes.parent_id(node.id());
                        if parent_id != node.id() {
                            let parent = nodes.get_node(parent_id);
                            if let oxc_ast::AstKind::MethodDefinition(method) = parent.kind() {
                                let method_name = &source[method.key.span().start as usize..method.key.span().end as usize];
                                (Some(method_name.to_string()), method.span.start as usize)
                            } else {
                                continue;
                            }
                        } else {
                            continue;
                        }
                    } else {
                        (name.map(|n| n.to_string()), func.span.start as usize)
                    }
                }
                oxc_ast::AstKind::VariableDeclarator(decl) => {
                    // Only if the value is an arrow function or function expression
                    let is_fn_value = decl.init.as_ref().is_some_and(|init| {
                        matches!(
                            init,
                            oxc_ast::ast::Expression::ArrowFunctionExpression(_)
                                | oxc_ast::ast::Expression::FunctionExpression(_)
                        )
                    });
                    if !is_fn_value {
                        continue;
                    }
                    let name = match &decl.id {
                        oxc_ast::ast::BindingPattern::BindingIdentifier(id) => {
                            Some(id.name.as_str().to_string())
                        }
                        _ => continue,
                    };
                    // Anchor is the parent VariableDeclaration for preceding comment
                    let parent_id = nodes.parent_id(node.id());
                    let parent = nodes.get_node(parent_id);
                    let anchor_start = match parent.kind() {
                        oxc_ast::AstKind::VariableDeclaration(vd) => vd.span.start as usize,
                        _ => decl.span.start as usize,
                    };
                    (name, anchor_start)
                }
                _ => continue,
            };

            let Some(name) = name else { continue };

            // Find the closest preceding comment
            let Some(comment) = find_preceding_comment(comments, anchor_start, source) else {
                continue;
            };

            let comment_start = comment.span.start as usize;
            let comment_end = comment.span.end as usize;
            let Some(comment_text) = source.get(comment_start..comment_end) else {
                continue;
            };

            // Skip JSDoc
            if comment_text.starts_with("/**") {
                continue;
            }

            let body = strip_comment_markers(comment_text);
            if !looks_like_paraphrase(&name, &body, max_comment_tokens, overlap_threshold) {
                continue;
            }

            let (line, column) = byte_offset_to_line_col(source, comment_start);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "comment-paraphrases-code".into(),
                message: format!(
                    "Comment above `{name}` paraphrases the function name. Rewrite to \
                     explain WHY (what breaks if this is deleted?), not WHAT — or delete \
                     the comment if no consequence comes to mind."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

use oxc_span::GetSpan;

/// Find the comment that immediately precedes the given byte offset.
/// The comment must end on the line directly before the anchor.
fn find_preceding_comment<'a>(
    comments: &'a [oxc_ast::Comment],
    anchor_start: usize,
    source: &str,
) -> Option<&'a oxc_ast::Comment> {
    // Find the closest comment that ends before anchor_start
    let mut best: Option<&oxc_ast::Comment> = None;
    for c in comments {
        let end = c.span.end as usize;
        if end > anchor_start {
            continue;
        }
        // Check that between comment end and anchor start there's only whitespace
        let between = &source[end..anchor_start];
        if between.trim().is_empty() {
            // Check it's on the immediately preceding line(s)
            let newlines = between.chars().filter(|&ch| ch == '\n').count();
            if newlines <= 1 {
                match best {
                    None => best = Some(c),
                    Some(prev) => {
                        if c.span.end > prev.span.end {
                            best = Some(c);
                        }
                    }
                }
            }
        }
    }
    best
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
    let overlap_f = overlap as f32;
    // comply-ignore: rust-no-lossy-as-cast — bounded count.
    let total_f = comment_tokens.len() as f32;
    let ratio = overlap_f / total_f;
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
    body.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_ascii_lowercase())
        .filter(|w| !STOP_WORDS.contains(&w.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
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
        let source =
            "// Avoid double-fire when the user double-clicks fast\nfunction handleClick() {}";
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
        assert_eq!(
            tokenize_identifier("fetch_user_data"),
            vec!["fetch", "user", "data"]
        );
        assert_eq!(
            tokenize_identifier("HTTPClient"),
            vec!["h", "t", "t", "p", "client"]
        );
    }


    #[test]
    fn unit_tokenize_comment() {
        assert_eq!(
            tokenize_comment("the handle click"),
            vec!["handle", "click"]
        );
        assert_eq!(
            tokenize_comment("a fetch and a parse"),
            vec!["fetch", "parse"]
        );
    }
}
