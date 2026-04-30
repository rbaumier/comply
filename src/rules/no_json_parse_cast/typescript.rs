//! no-json-parse-cast backend — reject `JSON.parse(x) as T`.
//!
//! Why: `JSON.parse` returns `any` (morally `unknown`), and immediately
//! casting the result to a typed shape is a lie. If the JSON doesn't match
//! the type, the lie crashes far from the origin. The correct approach is
//! to validate with a type guard or Zod schema at the boundary.
//!
//! Detection: walk `as_expression` nodes whose value side is a
//! `call_expression` whose callee is `JSON.parse`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["as_expression"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(inner) = node.named_child(0) else {
            return;
        };
        if inner.kind() != "call_expression" {
            return;
        }
        let Some(callee) = inner.child_by_field_name("function") else {
            return;
        };
        let Ok(callee_text) = callee.utf8_text(source_bytes) else {
            return;
        };
        if callee_text != "JSON.parse" {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-json-parse-cast".into(),
            message: "Casting `JSON.parse(...) as T` is a lie — the \
                      runtime shape may not match T. Validate with a \
                      Zod schema (`Schema.safeParse(JSON.parse(raw))`) \
                      or a type guard function that inspects the value."
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
    fn flags_json_parse_as_type() {
        assert_eq!(run_on("const u = JSON.parse(raw) as User;").len(), 1);
    }

    #[test]
    fn allows_json_parse_with_schema() {
        assert!(run_on("const u = UserSchema.parse(JSON.parse(raw));").is_empty());
    }

    #[test]
    fn allows_other_cast() {
        assert!(run_on("const u = value as User;").is_empty());
    }

    #[test]
    fn does_not_flag_other_function_call_cast() {
        assert!(run_on("const u = getRaw() as User;").is_empty());
    }
}
