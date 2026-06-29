//! no-commented-out-code — Rust backend.
//!
//! Same strategy as the TS backend: walk `line_comment` and
//! `block_comment` nodes, group adjacent ones, strip delimiters,
//! re-parse as Rust. A clean inner parse with at least one rich
//! construct (let declaration, call, macro, control flow) means the
//! comment is very likely commented-out code.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

/// State accumulated across visits: positions and text of every comment
/// node, in document order. We store owned `String` and row/col rather
/// than `Node<'_>` so the state can outlive the tree borrow.
#[derive(Default)]
struct State {
    comments: Vec<CommentSpan>,
}

struct CommentSpan {
    row: usize,
    col: usize,
    end_row: usize,
    text: String,
}

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["line_comment", "block_comment"])
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new(State::default()))
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(state) = state.and_then(|s| s.downcast_mut::<State>()) else {
            return;
        };
        let source_bytes = ctx.source.as_bytes();
        let Ok(text) = node.utf8_text(source_bytes) else {
            return;
        };
        // A comment that is a direct child of a `macro_rules!` definition
        // documents the following arm's invocation syntax
        // (`// log!(target: ..., Level::Info, ...)`), the canonical Rust macro
        // style — embedded usage documentation, not commented-out code. Comments
        // nested deeper (inside an arm's expansion body) stay checked.
        if node
            .parent()
            .is_some_and(|p| p.kind() == "macro_definition")
        {
            return;
        }
        let pos = node.start_position();
        let end = node.end_position();
        state.comments.push(CommentSpan {
            row: pos.row,
            col: pos.column,
            end_row: end.row,
            text: text.to_string(),
        });
    }

    fn finish(
        &self,
        ctx: &CheckCtx,
        state: Option<Box<dyn std::any::Any>>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(state) = state.and_then(|s| s.downcast::<State>().ok()) else {
            return;
        };
        let mut comments = state.comments;
        comments.sort_by_key(|c| (c.row, c.col));

        let groups = group_adjacent_spans(&comments);
        for group in groups {
            let Some(body) = build_group_body(&group) else {
                continue;
            };
            if !super::has_code_shape(&body) {
                continue;
            }
            if !parses_as_rust_code(&body) {
                continue;
            }
            let first = group.first().expect("group is non-empty");
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: first.row + 1,
                column: first.col + 1,
                rule_id: "no-commented-out-code".into(),
                message: "This comment looks like commented-out code — \
                          delete it. Git history preserves the original."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }
    }
}

/// Group adjacent comment spans. Mirrors `super::group_adjacent` but
/// for `CommentSpan` rather than `tree_sitter::Node`. Two comments are
/// considered adjacent if the second one starts on the same line as,
/// or the line immediately after, the first one ends.
fn group_adjacent_spans(comments: &[CommentSpan]) -> Vec<Vec<&CommentSpan>> {
    let mut groups: Vec<Vec<&CommentSpan>> = Vec::new();
    for c in comments {
        let extend = groups
            .last()
            .and_then(|g| g.last())
            .is_some_and(|last: &&CommentSpan| c.row <= last.end_row + 1);
        if extend {
            groups.last_mut().expect("last group exists").push(c);
        } else {
            groups.push(vec![c]);
        }
    }
    groups
}

fn build_group_body(group: &[&CommentSpan]) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    for c in group {
        let Some(stripped) = super::strip_comment_syntax(&c.text) else {
            continue;
        };
        if !stripped.trim().is_empty() {
            lines.push(stripped);
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

/// Re-parse `body` with the Rust grammar. Wraps the body in
/// `fn __probe__() { ... }` because most commented-out Rust fragments
/// are statements — they only make sense inside a function body, not
/// at module scope. A top-level `let` is a hard parse error; wrapped,
/// it's a legal `let_declaration`.
pub(super) fn parses_as_rust_code(body: &str) -> bool {
    let wrapped = format!("fn __probe__() {{\n{body}\n}}");
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .is_err()
    {
        return false;
    }
    let Some(tree) = parser.parse(&wrapped, None) else {
        return false;
    };
    let root = tree.root_node();
    if root.has_error() {
        return false;
    }
    // Scan only inside the wrapper body — the artificial `fn __probe__` itself
    // is a `function_item` (a "rich" kind) and must not count as
    // commented-out code.
    let Some(func) = root.child(0).filter(|n| n.kind() == "function_item") else {
        return false;
    };
    let Some(block) = func.child_by_field_name("body") else {
        return false;
    };
    contains_rich_code(block)
}

/// Return `true` if `node`'s subtree contains a rich Rust construct (a
/// `let`, call, macro, control-flow expression, item, …). Called on the
/// probe body block, so a nested commented-out construct is found while
/// the synthetic wrapper that holds it is never inspected.
fn contains_rich_code(node: tree_sitter::Node) -> bool {
    if is_rich_rust_kind(node.kind()) {
        return true;
    }
    let mut cursor = node.walk();
    node.children(&mut cursor).any(contains_rich_code)
}

fn is_rich_rust_kind(kind: &str) -> bool {
    matches!(
        kind,
        "let_declaration"
            | "call_expression"
            | "macro_invocation"
            | "assignment_expression"
            | "compound_assignment_expr"
            | "function_item"
            | "struct_item"
            | "enum_item"
            | "impl_item"
            | "trait_item"
            | "if_expression"
            | "match_expression"
            | "for_expression"
            | "while_expression"
            | "loop_expression"
            | "return_expression"
            | "use_declaration"
            | "break_expression"
            | "continue_expression"
            | "try_expression"
            | "await_expression"
            | "unary_expression"
            | "binary_expression"
            | "field_expression"
            | "index_expression"
            | "closure_expression"
    )
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

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_commented_let() {
        assert_eq!(run("// let x = 5;").len(), 1);
    }

    #[test]
    fn flags_commented_macro_call() {
        assert_eq!(run(r#"// println!("hello {}", x);"#).len(), 1);
    }

    #[test]
    fn flags_adjacent_commented_lines() {
        let src = "// let x = 5;\n// let y = 10;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_prose_comment() {
        assert!(run("// This function computes the total cost.").is_empty());
    }

    #[test]
    fn allows_triple_slash_doc() {
        assert!(run("/// Returns the parsed result.").is_empty());
    }

    #[test]
    fn allows_inner_module_doc() {
        assert!(run("//! Module-level documentation.").is_empty());
    }

    #[test]
    fn allows_pattern_list_prose() {
        // User's reported false positive, ported to Rust idioms.
        assert!(run("// let foo =, const foo =, static foo =").is_empty());
    }

    #[test]
    fn allows_short_label() {
        assert!(run("// setup").is_empty());
    }

    #[test]
    fn flags_commented_block_comment() {
        assert_eq!(run("/* let x = 5; foo(x); */").len(), 1);
    }

    #[test]
    fn allows_block_doc_comment() {
        assert!(run("/** doc */").is_empty());
    }

    #[test]
    fn allows_arm_documenting_comments_in_macro_rules() {
        // Issue #6344: comments preceding each `macro_rules!` arm document the
        // arm's invocation syntax — canonical Rust macro style, not dead code.
        let src = r#"
macro_rules! log {
    // log!(logger: my_logger, target: "my_target", Level::Info, "a {} event", "log");
    (logger: $logger:expr, target: $target:expr, $lvl:expr, $($arg:tt)+) => {{ () }};
    // log!(logger: my_logger, Level::Info, "a log event")
    (logger: $logger:expr, $lvl:expr, $($arg:tt)+) => {{ () }};
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_commented_code_outside_macro_rules() {
        // The macro_rules! guard must stay scoped: a commented-out statement at
        // module scope (no `macro_definition` ancestor) is still flagged.
        let src = r#"
macro_rules! log {
    () => {{ () }};
}
// let x = compute_value(a, b);
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_commented_code_in_function_body() {
        let src = "fn main() {\n    // let x = compute_value(a, b);\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_commented_code_in_macro_expansion_body() {
        // The guard exempts only comments that are direct children of the
        // `macro_rules!` definition (arm documentation). Commented-out code
        // nested inside an arm's expansion body is still flagged.
        let src = r#"
macro_rules! m {
    () => {{
        // let x = compute_value(a, b);
        ()
    }};
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_string_literal_shape_comment() {
        // #6348: a bare string literal documenting an assembled string is not
        // commented-out code, even though it contains code-shape characters.
        assert!(run(r#"// "{msg} ({lhs} vs {rhs})""#).is_empty());
    }

    #[test]
    fn allows_string_literal_with_semicolons() {
        assert!(run(r#"// "a; b; c""#).is_empty());
    }

    #[test]
    fn flags_nested_commented_function() {
        // Scanning inside the probe body still catches an inner `function_item`
        // / `call_expression`, so the fix does not blanket-exempt functions.
        assert_eq!(run("// fn helper() { do_thing(); }").len(), 1);
    }
}
