//! vue-watch-immediate-over-onmounted AST backend.
//!
//! Flags a Vue SFC that pairs a `watch(source, cb)` with an `onMounted`
//! whose callback body does nothing but call the same watch callback(s).
//! Such an `onMounted` only replays the watch on mount, so it can be
//! folded into `watch(source, cb, { immediate: true })`.
//!
//! The Vue grammar exposes a `<script>` body as opaque `raw_text`, so each
//! block is re-parsed with the TypeScript grammar. Watch-callback names are
//! collected from every `watch(src, cb)` whose `cb` is a plain identifier;
//! an `onMounted` is flagged only when EVERY statement of its callback body
//! is a call to one of those names. An `onMounted` that runs any other
//! statement (an extra side effect, an assignment, a branch) is left alone —
//! dropping it would lose that work.

use std::collections::HashSet;

use tree_sitter::Node;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::vue_sfc::{self, ScriptBlock};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["onMounted"])
    }

    fn check(&self, ctx: &CheckCtx, tree: &tree_sitter::Tree) -> Vec<Diagnostic> {
        let src = ctx.source;
        if !src.contains("onMounted(") || !src.contains("watch(") {
            return Vec::new();
        }

        // The Vue grammar keeps script bodies opaque; re-parse each block as
        // TypeScript so the onMounted callback body can be walked structurally.
        let parsed: Vec<(ScriptBlock<'_>, tree_sitter::Tree)> =
            vue_sfc::extract_scripts(tree, src)
                .into_iter()
                .filter_map(|block| parse_typescript(block.text).map(|t| (block, t)))
                .collect();

        // A `watch` and the `onMounted` that mirrors it may live in different
        // `<script>` blocks, so gather every watch-callback name first.
        let mut watch_fns: HashSet<String> = HashSet::new();
        for (block, ts_tree) in &parsed {
            collect_watch_callbacks(ts_tree.root_node(), block.text.as_bytes(), &mut watch_fns);
        }
        if watch_fns.is_empty() {
            return Vec::new();
        }

        let mut diagnostics = Vec::new();
        for (block, ts_tree) in &parsed {
            flag_onmounted(ts_tree.root_node(), block, &watch_fns, ctx, &mut diagnostics);
        }
        diagnostics
    }
}

fn parse_typescript(text: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .ok()?;
    parser.parse(text, None)
}

/// Record the callback name of every `watch(source, cb)` whose `cb` is a plain
/// identifier. An inline arrow/function callback has no name to fold into an
/// `{ immediate: true }` option, so it is skipped.
fn collect_watch_callbacks(root: Node, src: &[u8], out: &mut HashSet<String>) {
    for_each_node(root, |node| {
        if node.kind() != "call_expression" || callee_identifier(node, src) != Some("watch") {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        let arg_list = named_children_no_comments(args);
        let Some(cb) = arg_list.get(1) else {
            return;
        };
        if cb.kind() == "identifier"
            && let Ok(name) = cb.utf8_text(src)
        {
            out.insert(name.to_string());
        }
    });
}

/// Emit a diagnostic for each `onMounted` whose callback body is composed
/// entirely of matched watch-callback calls.
fn flag_onmounted(
    root: Node,
    block: &ScriptBlock<'_>,
    watch_fns: &HashSet<String>,
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let src = block.text.as_bytes();
    for_each_node(root, |node| {
        if node.kind() != "call_expression" || callee_identifier(node, src) != Some("onMounted") {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        let Some(callback) = named_children_no_comments(args).into_iter().next() else {
            return;
        };
        let Some(fname) = onmounted_body_watch_call(callback, src, watch_fns) else {
            return;
        };

        let pos = node.start_position();
        let line = pos.row + block.start_row + 1;
        let column = if pos.row == 0 {
            pos.column + block.start_column + 1
        } else {
            pos.column + 1
        };
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`onMounted` duplicates the `watch` — pass `{{ immediate: true }}` to the watch of `{fname}` instead."
            ),
            severity: Severity::Error,
            span: None,
        });
    });
}

/// The matched watch-callback name when EVERY statement of the `onMounted`
/// callback body is a call to a watch callback, else `None`.
///
/// Handles both a block body (`() => { a(); b(); }`) and an expression body
/// (`() => a()`). A body with zero statements, or any statement that is not a
/// matched watch-callback call, disqualifies the `onMounted`.
fn onmounted_body_watch_call(
    callback: Node,
    src: &[u8],
    watch_fns: &HashSet<String>,
) -> Option<String> {
    let body = callback.child_by_field_name("body")?;
    if body.kind() != "statement_block" {
        // Expression-body arrow: the body is the single statement.
        return matched_watch_call(body, src, watch_fns);
    }

    let statements = named_children_no_comments(body);
    if statements.is_empty() {
        return None;
    }
    let mut matched = None;
    for statement in statements {
        if statement.kind() != "expression_statement" {
            return None;
        }
        let expr = named_children_no_comments(statement).into_iter().next()?;
        let name = matched_watch_call(expr, src, watch_fns)?;
        matched.get_or_insert(name);
    }
    matched
}

/// The callee name when `expr` is a call to a plain identifier that is a
/// matched watch callback (arguments are irrelevant), else `None`.
fn matched_watch_call(expr: Node, src: &[u8], watch_fns: &HashSet<String>) -> Option<String> {
    if expr.kind() != "call_expression" {
        return None;
    }
    let name = callee_identifier(expr, src)?;
    watch_fns.contains(name).then(|| name.to_string())
}

/// The callee text of a `call_expression` when the callee is a plain
/// identifier (`foo(...)`), not a member/computed call (`a.foo(...)`).
fn callee_identifier<'src>(call: Node, src: &'src [u8]) -> Option<&'src str> {
    let func = call.child_by_field_name("function")?;
    if func.kind() != "identifier" {
        return None;
    }
    func.utf8_text(src).ok()
}

/// Named children of `node` with comment nodes filtered out, so a `/* … */`
/// between statements or arguments doesn't count as one.
fn named_children_no_comments(node: Node) -> Vec<Node> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor)
        .filter(|n| n.kind() != "comment")
        .collect()
}

/// Depth-first visit of every node under `root`.
fn for_each_node(root: Node, mut f: impl FnMut(Node)) {
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        f(node);
        cursor.reset(node);
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        let tree = parser.parse(source, None).expect("parser");
        Check.check(&CheckCtx::for_test(Path::new("t.vue"), source), &tree)
    }

    #[test]
    fn flags_onmounted_duplicating_watch() {
        let sfc = "<script setup>\nwatch(x, load)\nonMounted(() => load(x.value))\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_watch_with_immediate() {
        let sfc = "<script setup>\nwatch(x, load, { immediate: true })\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_onmounted_unrelated() {
        let sfc = "<script setup>\nwatch(x, load)\nonMounted(() => otherThing())\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_onmounted_with_extra_side_effect() {
        // Repro from the issue: the body calls the watch callback AND an extra
        // `createTimer()`; dropping the onMounted would lose the timer setup.
        let sfc = "<script setup>\nonMounted(() => {\n  updateSlaStatus();\n  createTimer();\n})\nwatch(() => props.chat, updateSlaStatus)\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn allows_onmounted_with_non_call_statement() {
        let sfc = "<script setup>\nonMounted(() => {\n  updateSlaStatus();\n  const x = 1;\n})\nwatch(src, updateSlaStatus)\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn flags_single_matched_call_block_body() {
        let sfc = "<script setup>\nonMounted(() => {\n  updateSlaStatus();\n})\nwatch(src, updateSlaStatus)\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn flags_expression_body_arrow() {
        let sfc =
            "<script setup>\nonMounted(() => updateSlaStatus())\nwatch(src, updateSlaStatus)\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn flags_multiple_statements_all_matched_watch_callbacks() {
        let sfc = "<script setup>\nonMounted(() => {\n  a();\n  b();\n})\nwatch(s1, a)\nwatch(s2, b)\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_onmounted_call_to_non_watch_callback() {
        let sfc = "<script setup>\nonMounted(() => nonWatch())\nwatch(src, updateSlaStatus)\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn flags_watch_and_onmounted_in_separate_script_blocks() {
        // The watch and its mirror onMounted live in different `<script>`
        // blocks; watch-callback names are gathered across every block.
        let sfc = "<script>\nwatch(src, updateSlaStatus)\n</script>\n<script setup>\nonMounted(() => updateSlaStatus())\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }
}
