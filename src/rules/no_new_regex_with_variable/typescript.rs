//! no-new-regex-with-variable backend — flag `new RegExp(variable)`.
//!
//! Why: user-controlled regex patterns open the door to ReDoS (Regular
//! Expression Denial of Service) — a crafted pattern can make the regex
//! engine backtrack exponentially, freezing the event loop. Even if the
//! input isn't attacker-controlled today, it might be tomorrow. Use a
//! literal regex or a vetted safe-regex library.
//!
//! Detection: walk `new_expression` nodes whose constructor is `RegExp`
//! and whose single argument is an identifier or expression (not a string
//! literal).

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["new_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(constructor) = node.child_by_field_name("constructor") else {
            return;
        };
        let Ok(name) = constructor.utf8_text(source_bytes) else {
            return;
        };
        if name != "RegExp" {
            return;
        }
        let Some(args) = node.child_by_field_name("arguments") else {
            return;
        };
        let Some(first_arg) = args.named_child(0) else {
            return;
        };
        // String literal is safe — flag everything else.
        if matches!(first_arg.kind(), "string" | "template_string") {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: ctx.path.to_path_buf(),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-new-regex-with-variable".into(),
            message: "`new RegExp(variable)` — ReDoS risk. A crafted \
                      pattern can freeze the event loop via exponential \
                      backtracking. Use a literal regex or a vetted \
                      safe-regex library."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn run_on(source: &str) -> Vec<Diagnostic> {


        crate::rules::test_helpers::run_ts(source, &Check)


    }

    #[test]
    fn flags_regex_with_variable() {
        assert_eq!(run_on("const r = new RegExp(userInput);").len(), 1);
    }

    #[test]
    fn allows_regex_with_string_literal() {
        assert!(run_on("const r = new RegExp('foo[a-z]+');").is_empty());
    }

    #[test]
    fn allows_literal_regex() {
        assert!(run_on("const r = /foo/g;").is_empty());
    }
}
