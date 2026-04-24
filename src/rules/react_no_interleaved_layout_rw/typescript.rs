//! AST backend for react-no-interleaved-layout-rw.
//!
//! For every function body, walk its statements in document order and
//! collect:
//! - layout reads: `foo.offsetWidth`, `foo.offsetHeight`, `foo.clientWidth`,
//!   `foo.clientHeight`, `foo.scrollTop`, `foo.scrollHeight`,
//!   `foo.getBoundingClientRect(...)`,
//! - style writes: assignments where the target is `foo.style.x`.
//!
//! The rule fires once per function if at least one write appears
//! between two reads (or a read appears between two writes) — i.e. the
//! reads and writes are not separated.

use crate::diagnostic::{Diagnostic, Severity};

const LAYOUT_READ_PROPS: &[&str] = &[
    "offsetWidth",
    "offsetHeight",
    "offsetTop",
    "offsetLeft",
    "clientWidth",
    "clientHeight",
    "scrollTop",
    "scrollLeft",
    "scrollWidth",
    "scrollHeight",
    "getBoundingClientRect",
    "getClientRects",
];

#[derive(Clone, Copy, PartialEq)]
enum Op {
    Read,
    Write,
}

fn is_layout_read(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if node.kind() == "member_expression" {
        let Some(prop) = node.child_by_field_name("property") else { return false };
        let Ok(name) = prop.utf8_text(source) else { return false };
        return LAYOUT_READ_PROPS.contains(&name);
    }
    false
}

fn is_style_write(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    // assignment_expression with target `x.style.y`.
    if node.kind() != "assignment_expression" {
        return false;
    }
    let Some(left) = node.child_by_field_name("left") else { return false };
    if left.kind() != "member_expression" {
        return false;
    }
    let Some(object) = left.child_by_field_name("object") else { return false };
    if object.kind() != "member_expression" {
        return false;
    }
    let Some(inner_prop) = object.child_by_field_name("property") else { return false };
    inner_prop.utf8_text(source).ok() == Some("style")
}

fn collect_ops(node: tree_sitter::Node<'_>, source: &[u8], ops: &mut Vec<Op>) {
    if is_layout_read(node, source) {
        ops.push(Op::Read);
    }
    if is_style_write(node, source) {
        ops.push(Op::Write);
    }
    // Do not descend into nested functions — their ops belong to their
    // own scope, not the enclosing function's frame.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "method_definition" => continue,
            _ => collect_ops(child, source, ops),
        }
    }
}

fn is_interleaved(ops: &[Op]) -> bool {
    // Interleaved if after compressing consecutive identical ops we
    // have 3+ runs, OR one run of reads followed by writes then reads
    // again (any ABA-like pattern).
    if ops.len() < 3 {
        return false;
    }
    let mut runs = 1;
    for w in ops.windows(2) {
        if w[0] != w[1] {
            runs += 1;
        }
    }
    runs >= 3
}

crate::ast_check! { |node, source, ctx, diagnostics|
    let _ = ctx;
    if !matches!(
        node.kind(),
        "function_declaration"
            | "function_expression"
            | "arrow_function"
            | "method_definition"
    ) {
        return;
    }
    let Some(body) = node.child_by_field_name("body") else { return };
    let mut ops = Vec::new();
    collect_ops(body, source, &mut ops);
    if !is_interleaved(&ops) {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Layout reads (e.g. `offsetWidth`, `getBoundingClientRect`) interleaved \
         with `.style.*` writes force sync layout. Batch reads first, writes second."
            .into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_tsx(source, &Check)
    }

    #[test]
    fn flags_interleaved_read_write_read() {
        let src = r#"
function thrash() {
  const w = el.offsetWidth;
  el.style.width = w + "px";
  const h = el.offsetHeight;
}
"#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_reads_then_writes() {
        let src = r#"
function tidy() {
  const w = el.offsetWidth;
  const h = el.offsetHeight;
  el.style.width = w + "px";
  el.style.height = h + "px";
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_only_reads() {
        let src = r#"
function r() {
  const a = el.offsetWidth;
  const b = el.offsetHeight;
  return a + b;
}
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_only_writes() {
        let src = r#"
function w() {
  el.style.width = "1px";
  el.style.height = "2px";
}
"#;
        assert!(run(src).is_empty());
    }
}
