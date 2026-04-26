//! hono-missing-secure-headers backend — Hono app without secureHeaders().

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

#[derive(Debug)]
pub struct Check;

/// `(has_routes, hono_line)` accumulated across the visit.
type State = (bool, Option<usize>);

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["new_expression", "call_expression"])
    }

    fn create_state(&self) -> Option<Box<dyn std::any::Any>> {
        Some(Box::new((false, None::<usize>)))
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        state: Option<&mut dyn std::any::Any>,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let st = state.unwrap().downcast_mut::<State>().unwrap();

        // Look for `new Hono()` for the report location.
        if node.kind() == "new_expression"
            && let Some(constructor) = node.child_by_field_name("constructor")
            && constructor.utf8_text(source_bytes).unwrap_or("") == "Hono"
            && st.1.is_none()
        {
            st.1 = Some(node.start_position().row + 1);
        }
        // Look for route method calls.
        if node.kind() == "call_expression"
            && let Some(callee) = node.child_by_field_name("function")
            && callee.kind() == "member_expression"
            && let Some(method) = callee.child_by_field_name("property")
        {
            let name = method.utf8_text(source_bytes).unwrap_or("");
            if matches!(name, "get" | "post" | "put" | "delete" | "patch") {
                st.0 = true;
            }
        }
    }

    fn finish(
        &self,
        ctx: &CheckCtx,
        state: Option<Box<dyn std::any::Any>>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Only check Hono files.
        if !ctx.source.contains("from 'hono'") && !ctx.source.contains("from \"hono\"") {
            return;
        }
        // Skip if secureHeaders is already imported.
        if ctx.source.contains("hono/secure-headers") {
            return;
        }
        let st = state.unwrap().downcast::<State>().unwrap();
        let (has_routes, hono_line) = *st;
        if !has_routes {
            return;
        }
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: hono_line.unwrap_or(1),
            column: 1,
            rule_id: "hono-missing-secure-headers".into(),
            message: "Hono app defines routes without `secureHeaders()` middleware — security headers are missing.".into(),
            severity: Severity::Warning,
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
    fn flags_hono_app_without_secure_headers() {
        let src = "import { Hono } from 'hono';\nconst app = new Hono();\napp.get('/', handler);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_hono_app_with_secure_headers() {
        let src = "import { Hono } from 'hono';\nimport { secureHeaders } from 'hono/secure-headers';\nconst app = new Hono();\napp.use(secureHeaders());\napp.get('/', handler);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_non_hono_files() {
        let src = "app.get('/', handler);";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn ignores_hono_import_without_routes() {
        let src = "import { Hono } from 'hono';\nconst app = new Hono();";
        assert!(run_on(src).is_empty());
    }
}
