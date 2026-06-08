//! no-catch-without-use backend — flag `catch (e)` whose binding `e` is
//! never referenced inside the catch body.
//!
//! Detection: every `catch_clause` that has a `parameter` field gets its
//! body scanned. If no `identifier` node matching the parameter name is
//! found in the body, emit a diagnostic. Destructuring (`catch ({ code })`)
//! is skipped — the destructured names are already "used" by being bound.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};

fn body_references(body: tree_sitter::Node, name: &str, source: &[u8]) -> bool {
    let mut cursor = body.walk();
    let mut stack: Vec<tree_sitter::Node> = body.children(&mut cursor).collect();
    while let Some(n) = stack.pop() {
        if n.kind() == "identifier" && n.utf8_text(source).unwrap_or("") == name {
            return true;
        }
        let mut c = n.walk();
        for child in n.children(&mut c) {
            stack.push(child);
        }
    }
    false
}

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["catch_clause"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source = ctx.source.as_bytes();
        let Some(param) = node.child_by_field_name("parameter") else {
            return; // bare `catch { ... }` — nothing to flag.
        };
        // Only handle simple identifier bindings. Destructuring patterns
        // (`catch ({ code })`) are out of scope — the bound names are
        // "used" by construction, and matching them would be noisy.
        if param.kind() != "identifier" {
            return;
        }
        let Ok(name) = param.utf8_text(source) else {
            return;
        };
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        if body_references(body, name, source) {
            return;
        }
        let pos = param.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "no-catch-without-use".into(),
            message: format!(
                "`catch ({name})` is never used — drop the binding (`catch {{ ... }}`) \
                 or reference `{name}` in the handler."
            ),
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
    fn flags_unused_binding() {
        let d = run_on("try { x(); } catch (e) { return null; }");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-catch-without-use");
    }

    #[test]
    fn flags_unused_binding_with_log() {
        let d = run_on("try { x(); } catch (e) { console.log('oops'); }");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_used_binding() {
        assert!(run_on("try { x(); } catch (e) { console.log(e); }").is_empty());
    }

    #[test]
    fn allows_rethrow() {
        assert!(run_on("try { x(); } catch (e) { throw e; }").is_empty());
    }

    #[test]
    fn allows_bare_catch() {
        assert!(run_on("try { x(); } catch { return null; }").is_empty());
    }

    #[test]
    fn allows_destructured_binding() {
        // Destructuring is skipped — conservative.
        assert!(run_on("try { x(); } catch ({ code }) { return null; }").is_empty());
    }

    #[test]
    fn allows_used_in_nested_expr() {
        assert!(run_on("try { x(); } catch (e) { return new Error(e.message); }").is_empty());
    }
}
