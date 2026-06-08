//! testing-no-stubglobal-without-restore OXC backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Check if a call expression is `vi.<method>` where method is in `methods`.
fn is_vi_method(call: &oxc_ast::ast::CallExpression, source: &str, methods: &[&str]) -> bool {
    let oxc_ast::ast::Expression::StaticMemberExpression(member) = &call.callee else {
        return false;
    };
    let obj_span = member.object.span();
    let obj_text = &source[obj_span.start as usize..obj_span.end as usize];
    if obj_text != "vi" {
        return false;
    }
    methods.contains(&member.property.name.as_str())
}

/// Check if a call expression node is inside an `afterEach(...)` or `afterAll(...)` call.
fn is_inside_after_hook(
    node_id: oxc_semantic::NodeId,
    semantic: &oxc_semantic::Semantic,
    _source: &str,
) -> bool {
    let nodes = semantic.nodes();
    let mut current = node_id;
    loop {
        let parent_id = nodes.parent_id(current);
        if parent_id == current {
            // Reached root
            return false;
        }
        current = parent_id;
        let parent = nodes.get_node(current);
        if let AstKind::CallExpression(call) = parent.kind() {
            let oxc_ast::ast::Expression::Identifier(ident) = &call.callee else {
                continue;
            };
            if matches!(ident.name.as_str(), "afterEach" | "afterAll") {
                return true;
            }
        }
    }
}

/// Check whether there's a `vi.<method>()` call inside an afterEach/afterAll.
fn has_scoped_unstub(
    semantic: &oxc_semantic::Semantic,
    source: &str,
    method: &str,
) -> bool {
    for node in semantic.nodes().iter() {
        let AstKind::CallExpression(call) = node.kind() else {
            continue;
        };
        if !is_vi_method(call, source, &[method]) {
            continue;
        }
        if is_inside_after_hook(node.id(), semantic, source) {
            return true;
        }
    }
    false
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [oxc_ast::AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        let has_stub_global = ctx.source_contains("stubGlobal");
        let has_stub_env = ctx.source_contains("stubEnv");
        if !(has_stub_global || has_stub_env) {
            return diagnostics;
        }

        let unstubbed_globals = has_scoped_unstub(semantic, ctx.source, "unstubAllGlobals");
        let unstubbed_envs = has_scoped_unstub(semantic, ctx.source, "unstubAllEnvs");

        for node in semantic.nodes().iter() {
            let AstKind::CallExpression(call) = node.kind() else {
                continue;
            };

            if is_vi_method(call, ctx.source, &["stubGlobal"]) && !unstubbed_globals {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "vi.stubGlobal() without vi.unstubAllGlobals() in afterEach/afterAll leaks stubs into sibling tests.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
                continue;
            }

            if is_vi_method(call, ctx.source, &["stubEnv"]) && !unstubbed_envs {
                let (line, column) =
                    byte_offset_to_line_col(ctx.source, call.span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "vi.stubEnv() without vi.unstubAllEnvs() in afterEach/afterAll leaks env stubs into sibling tests.".into(),
                    severity: Severity::Warning,
                    span: None,
                });
            }
        }

        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(s, &Check)
    }


    #[test]
    fn flags_stub_global_without_restore() {
        assert_eq!(
            run("beforeEach(() => { vi.stubGlobal('fetch', vi.fn()); });").len(),
            1
        );
    }


    #[test]
    fn flags_stub_env_without_restore() {
        assert_eq!(
            run("beforeEach(() => { vi.stubEnv('NODE_ENV', 'test'); });").len(),
            1
        );
    }


    #[test]
    fn allows_stub_global_with_restore() {
        let src = "beforeEach(() => { vi.stubGlobal('fetch', vi.fn()); });\n\
                   afterEach(() => { vi.unstubAllGlobals(); });";
        assert!(run(src).is_empty());
    }


    #[test]
    fn allows_stub_env_with_restore() {
        let src = "beforeEach(() => { vi.stubEnv('NODE_ENV', 'test'); });\n\
                   afterEach(() => { vi.unstubAllEnvs(); });";
        assert!(run(src).is_empty());
    }


    #[test]
    fn flags_stub_global_even_if_envs_restored() {
        let src = "beforeEach(() => { vi.stubGlobal('fetch', vi.fn()); });\n\
                   afterEach(() => { vi.unstubAllEnvs(); });";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_unstub_at_top_level() {
        // unstubAllGlobals exists, but isn't inside afterEach/afterAll → still leaks.
        let src = "vi.unstubAllGlobals();\n\
                   beforeEach(() => { vi.stubGlobal('fetch', vi.fn()); });";
        assert_eq!(run(src).len(), 1);
    }


    #[test]
    fn flags_unstub_in_test_body() {
        let src = "beforeEach(() => { vi.stubEnv('NODE_ENV', 'test'); });\n\
                   test('a', () => { vi.unstubAllEnvs(); });";
        assert_eq!(run(src).len(), 1);
    }
}
