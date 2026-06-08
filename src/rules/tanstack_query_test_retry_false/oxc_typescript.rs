use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, ObjectPropertyKind, PropertyKey};
use std::sync::Arc;

pub struct Check;

fn is_test_file(path: &std::path::Path) -> bool {
    let s = path.to_string_lossy();
    s.ends_with(".test.ts")
        || s.ends_with(".test.tsx")
        || s.ends_with(".test.js")
        || s.ends_with(".test.jsx")
        || s.ends_with(".spec.ts")
        || s.ends_with(".spec.tsx")
        || s.ends_with(".spec.js")
        || s.ends_with(".spec.jsx")
        || s.contains("__tests__")
}

fn find_prop_value<'a>(
    props: &'a oxc_allocator::Vec<'a, ObjectPropertyKind<'a>>,
    key: &str,
) -> Option<&'a Expression<'a>> {
    for prop_kind in props {
        let ObjectPropertyKind::ObjectProperty(pair) = prop_kind else { continue };
        let name = match &pair.key {
            PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            PropertyKey::StringLiteral(s) => s.value.as_str(),
            _ => continue,
        };
        if name == key {
            return Some(&pair.value);
        }
    }
    None
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["QueryClient"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else { return };
        if !is_test_file(ctx.path) {
            return;
        }

        let Expression::Identifier(ctor) = &new_expr.callee else { return };
        if ctor.name.as_str() != "QueryClient" {
            return;
        }

        // Check: new QueryClient({ defaultOptions: { queries: { retry: false } } })
        if let Some(Argument::ObjectExpression(opts)) = new_expr.arguments.first()
            && let Some(Expression::ObjectExpression(defaults)) =
                find_prop_value(&opts.properties, "defaultOptions")
                && let Some(Expression::ObjectExpression(queries)) =
                    find_prop_value(&defaults.properties, "queries")
                    && let Some(Expression::BooleanLiteral(val)) =
                        find_prop_value(&queries.properties, "retry")
                        && !val.value {
                            return;
                        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Test-file `QueryClient` must set `defaultOptions.queries.retry: false` to keep tests deterministic.".into(),
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

    fn run_test(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "foo.test.ts")
    }

    fn run_nontest(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, src, "foo.ts")
    }

    #[test]
    fn flags_bare_new_query_client_in_test() {
        assert_eq!(run_test("const c = new QueryClient();").len(), 1);
    }

    #[test]
    fn flags_missing_retry_false_in_test() {
        let src = "const c = new QueryClient({ defaultOptions: { queries: { staleTime: 0 } } });";
        assert_eq!(run_test(src).len(), 1);
    }

    #[test]
    fn allows_retry_false_in_test() {
        let src = "const c = new QueryClient({ defaultOptions: { queries: { retry: false } } });";
        assert!(run_test(src).is_empty());
    }

    #[test]
    fn ignores_non_test_files() {
        assert!(run_nontest("const c = new QueryClient();").is_empty());
    }
}
