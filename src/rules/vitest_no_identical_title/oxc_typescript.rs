//! vitest-no-identical-title oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, Statement};
use rustc_hash::FxHashMap;
use std::sync::Arc;

pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.contains(".test.") || s.contains(".spec.") || s.contains("__tests__")
}

/// True when `call` looks like `test("title", …)` / `it("title", …)`. Also
/// matches `.only` / `.skip` variants. Returns `(title, callee_span_start)`.
fn extract_test_title<'a>(
    call: &'a oxc_ast::ast::CallExpression<'a>,
) -> Option<(&'a str, u32)> {
    let (name, span_start) = match &call.callee {
        Expression::Identifier(id) => (id.name.as_str(), id.span.start),
        Expression::StaticMemberExpression(m) => {
            let Expression::Identifier(obj) = &m.object else { return None };
            // Allow `.only`, `.skip`, `.todo`, `.concurrent`.
            let prop = m.property.name.as_str();
            if !matches!(prop, "only" | "skip" | "todo" | "concurrent") {
                return None;
            }
            (obj.name.as_str(), obj.span.start)
        }
        _ => return None,
    };
    if !matches!(name, "test" | "it") {
        return None;
    }
    let Argument::StringLiteral(lit) = call.arguments.first()? else {
        return None;
    };
    Some((lit.value.as_str(), span_start))
}

/// Collect titles from the immediate test/it calls inside a body of
/// statements (top-level or inside one describe block — we don't descend
/// into nested describes).
fn collect_titles<'a>(
    stmts: &'a [Statement<'a>],
    out: &mut FxHashMap<String, Vec<u32>>,
) {
    for stmt in stmts.iter() {
        let Statement::ExpressionStatement(es) = stmt else { continue };
        let Expression::CallExpression(call) = &es.expression else { continue };
        if let Some((title, span_start)) = extract_test_title(call) {
            out.entry(title.to_string()).or_default().push(span_start);
        }
    }
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Program, AstType::CallExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["test(", "it("])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if !is_test_file(ctx.path) {
            return;
        }
        let stmts: &[Statement] = match node.kind() {
            AstKind::Program(prog) => &prog.body,
            // Inside `describe("…", () => { … })`, collect titles from the
            // callback body — different describes have independent scopes.
            AstKind::CallExpression(call) => {
                let Expression::Identifier(id) = &call.callee else { return };
                if id.name.as_str() != "describe" {
                    return;
                }
                let Some(cb) = call.arguments.get(1) else { return };
                match cb {
                    Argument::ArrowFunctionExpression(a) => &a.body.statements,
                    Argument::FunctionExpression(f) => {
                        let Some(body) = &f.body else { return };
                        &body.statements
                    }
                    _ => return,
                }
            }
            _ => return,
        };
        let mut titles: FxHashMap<String, Vec<u32>> = FxHashMap::default();
        collect_titles(stmts, &mut titles);
        for (title, positions) in titles.iter() {
            if positions.len() < 2 {
                continue;
            }
            // Flag every duplicate past the first.
            for span_start in positions.iter().skip(1) {
                let (line, column) = byte_offset_to_line_col(ctx.source, *span_start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "Duplicate test title \"{title}\" in this describe scope — \
                         the second run is silently merged in reporter output."
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
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
    
    fn run(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_with_ctx(&Check, src, "/tmp/foo.test.ts", &crate::project::ProjectCtx::for_test_with_framework(""), crate::rules::file_ctx::default_static_file_ctx())
    }

    #[test]
    fn flags_duplicate_top_level() {
        let src = r#"
            test("returns 200", () => {});
            test("returns 200", () => {});
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_distinct_titles() {
        let src = r#"
            test("returns 200", () => {});
            test("returns 401", () => {});
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_duplicate_inside_describe() {
        let src = r#"
            describe("auth", () => {
                test("rejects empty", () => {});
                test("rejects empty", () => {});
            });
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_same_title_in_different_describes() {
        let src = r#"
            describe("auth", () => { test("ok", () => {}); });
            describe("api", () => { test("ok", () => {}); });
        "#;
        assert!(run(src).is_empty());
    }
}
