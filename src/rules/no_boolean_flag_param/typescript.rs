//! no-boolean-flag-param backend — flag function parameters typed as boolean.
//!
//! Why: `sendNotification(msg, isUrgent: boolean)` should be two functions:
//! `sendUrgentNotification(msg)` and `sendNormalNotification(msg)`. A
//! boolean flag turns one function into two hidden behaviors, which:
//! - can't be independently tested without the flag's sibling branch
//! - forces readers to scan the body to understand what true vs false means
//! - makes call sites opaque (`sendNotification(msg, true)` — what's true?)
//!
//! Detection: walk `required_parameter` / `optional_parameter` nodes whose
//! `type_annotation` is `: boolean`. Functions (and methods) only — not
//! standalone variable declarations.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["required_parameter", "optional_parameter"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        if !inside_formal_parameters(node) {
            return;
        }
        if !has_boolean_annotation(node, source_bytes) {
            return;
        }
        let name = param_name(node, source_bytes).unwrap_or("<flag>");
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-boolean-flag-param".into(),
            message: format!(
                "Boolean parameter '{name}' controls a branch — split \
                 into two named functions instead. A ternary or options \
                 object is not a fix; the boolean must disappear from \
                 the signature entirely."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True if the parameter is directly inside a `formal_parameters` node —
/// rules out other uses of `required_parameter` in destructuring, etc.
fn inside_formal_parameters(node: tree_sitter::Node) -> bool {
    node.parent()
        .is_some_and(|p| p.kind() == "formal_parameters")
}

fn has_boolean_annotation(node: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() != "type_annotation" {
            continue;
        }
        let mut ta_cursor = child.walk();
        for gc in child.children(&mut ta_cursor) {
            if gc.kind() == "predefined_type"
                && gc.utf8_text(source).is_ok_and(|t| t.trim() == "boolean")
            {
                return true;
            }
        }
    }
    false
}

fn param_name<'a>(node: tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(source).ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_ts(source, &Check)


    }

    #[test]
    fn flags_boolean_param() {
        let diags = run_on("function send(msg: string, isUrgent: boolean) {}");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("'isUrgent'"));
    }

    #[test]
    fn flags_arrow_function_boolean_param() {
        assert_eq!(
            run_on("const f = (ready: boolean) => ready;").len(),
            1
        );
    }

    #[test]
    fn allows_non_boolean_params() {
        assert!(run_on("function f(a: number, b: string) {}").is_empty());
    }

    #[test]
    fn allows_boolean_variable_not_in_params() {
        assert!(run_on("const isReady: boolean = true;").is_empty());
    }

    #[test]
    fn flags_multiple_boolean_params() {
        assert_eq!(
            run_on("function f(isA: boolean, isB: boolean) {}").len(),
            2
        );
    }
}
