//! use-type-alias backend — detect repeated complex inline type annotations
//! via tree-sitter AST.
//!
//! Two-pass: first walk collects all union/intersection type annotation
//! strings and their line numbers, then reports duplicates.  The
//! `ast_check!` macro only supports per-node logic, so we implement
//! `AstCheck` manually.

use std::collections::HashMap;

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

/// True when the node is a complex type (union or intersection) that is
/// worth extracting into an alias.
fn is_complex_type(kind: &str) -> bool {
    kind == "union_type" || kind == "intersection_type"
}

#[derive(Default)]
struct State {
    annotation_lines: HashMap<String, Vec<usize>>,
}

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["union_type", "intersection_type"])
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
        let source = ctx.source.as_bytes();
        let Some(state) = state.and_then(|s| s.downcast_mut::<State>()) else {
            return;
        };
        // Skip nested union/intersection — only count the outermost.
        if let Some(parent) = node.parent()
            && is_complex_type(parent.kind())
        {
            return;
        }
        let text = match node.utf8_text(source) {
            Ok(t) => t,
            Err(_) => return,
        };
        // Must have at least 2 members to be worth aliasing.
        if text.len() <= 5 {
            return;
        }
        let line = node.start_position().row + 1;
        state
            .annotation_lines
            .entry(text.to_string())
            .or_default()
            .push(line);
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
        let start = diagnostics.len();
        for (annotation, lines) in &state.annotation_lines {
            if lines.len() >= 2 {
                for &line_num in lines {
                    diagnostics.push(Diagnostic {
                        path: std::sync::Arc::clone(&ctx.path_arc),
                        line: line_num,
                        column: 1,
                        rule_id: "use-type-alias".into(),
                        message: format!(
                            "Inline type `{}` appears {} times \u{2014} extract a type alias.",
                            annotation,
                            lines.len()
                        ),
                        severity: Severity::Warning,
                        span: None,
                    });
                }
            }
        }
        diagnostics[start..].sort_by_key(|d| d.line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_repeated_union_annotation() {
        let src = r#"
function foo(x: string | number) {}
function bar(y: string | number) {}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn flags_repeated_intersection_annotation() {
        let src = r#"
function foo(x: Foo & Bar) {}
function bar(y: Foo & Bar) {}
"#;
        assert_eq!(run_on(src).len(), 2);
    }

    #[test]
    fn allows_unique_annotations() {
        let src = r#"
function foo(x: string | number) {}
function bar(y: boolean | null) {}
"#;
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_simple_annotations() {
        let src = r#"
function foo(x: string) {}
function bar(y: string) {}
"#;
        assert!(run_on(src).is_empty());
    }
}
