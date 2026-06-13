use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::BindingPattern;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TryStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TryStatement(try_stmt) = node.kind() else { return };

        let Some(handler) = &try_stmt.handler else { return };
        let Some(param) = &handler.param else {
            return; // bare `catch { ... }` — nothing to flag.
        };
        // Only handle simple identifier bindings.
        let BindingPattern::BindingIdentifier(ident) = &param.pattern else {
            return;
        };
        let name = ident.name.as_str();

        // An underscore-prefixed name (`_`, `_e`, `_err`, …) is the
        // intentional-discard convention — the binding is deliberately unused.
        if name.starts_with('_') {
            return;
        }

        // Use semantic symbol info to check if the binding is referenced.
        if let Some(symbol_id) = ident.symbol_id.get() {
            let mut refs = semantic.symbol_references(symbol_id);
            if refs.next().is_some() {
                return; // has at least one reference — binding is used.
            }
        } else {
            // No symbol id — fallback to text check in the body.
            let body_src =
                &ctx.source[handler.body.span.start as usize..handler.body.span.end as usize];
            if body_src.contains(name) {
                return;
            }
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, ident.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_unused_binding() {
        assert_eq!(run("try { x(); } catch (e) { return null; }").len(), 1);
    }

    #[test]
    fn allows_used_binding() {
        assert!(run("try { x(); } catch (e) { console.log(e); }").is_empty());
    }

    #[test]
    fn allows_bare_underscore_discard() {
        // Issue #1787: `catch (_)` is the intentional-discard convention.
        assert!(
            run("try { const p = await req.json(); } catch (_) { return new Response('bad'); }")
                .is_empty()
        );
    }

    #[test]
    fn allows_underscore_prefixed_discard() {
        assert!(run("try { x(); } catch (_err) { return null; }").is_empty());
    }
}
