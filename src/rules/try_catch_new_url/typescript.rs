//! try-catch-new-url backend — flag `new URL(...)` not wrapped in a try.
//!
//! Detection: every `new_expression` whose constructor is `URL`, not
//! enclosed by a `try_statement` body within the same function boundary.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

const KINDS: &[&str] = &["new_expression"];

const FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "function",
    "arrow_function",
    "method_definition",
    "generator_function",
    "generator_function_declaration",
];

fn is_inside_try_body(node: tree_sitter::Node) -> bool {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "try_statement"
            && let Some(body) = n.child_by_field_name("body")
        {
            let ns = node.start_byte();
            let ne = node.end_byte();
            if ns >= body.start_byte() && ne <= body.end_byte() {
                return true;
            }
        }
        if FUNCTION_KINDS.contains(&n.kind()) {
            return false;
        }
        current = n.parent();
    }
    false
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["new URL"])
    }

    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(KINDS)
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        let Some(ctor) = node.child_by_field_name("constructor") else {
            return;
        };
        let Ok(ctor_name) = ctor.utf8_text(source) else {
            return;
        };
        if ctor_name != "URL" {
            return;
        }
        if is_inside_try_body(node) {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "try-catch-new-url".into(),
            message: "`new URL(...)` throws on invalid input — wrap in try/catch \
                      or gate with `URL.canParse(s)` first."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_bare_new_url() {
        let d = run_on("const u = new URL(input);");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "try-catch-new-url");
    }

    #[test]
    fn flags_new_url_in_fn() {
        let d = run_on("function f(s) { return new URL(s); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_inside_try() {
        assert!(run_on("try { const u = new URL(input); } catch (e) { log(e); }").is_empty());
    }

    #[test]
    fn allows_other_constructors() {
        assert!(run_on("const u = new MyUrl(input);").is_empty());
    }
}
