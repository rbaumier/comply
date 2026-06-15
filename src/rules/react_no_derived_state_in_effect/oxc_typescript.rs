//! react-no-derived-state-in-effect oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Root identifier names declared in a `useEffect` dependency array. For a
/// member-expression dependency like `item.path`, the root is `item`. Spread or
/// non-expression elements are ignored.
fn dependency_roots<'a>(deps: &'a oxc_ast::ast::ArrayExpression<'a>) -> Vec<&'a str> {
    deps.elements
        .iter()
        .filter_map(|el| el.as_expression())
        .filter_map(root_identifier)
        .collect()
}

/// The leftmost identifier of an expression usable as a dependency: `a` for `a`,
/// `a` for `a.b.c`. Returns `None` for anything else (literals, calls, ...).
fn root_identifier<'a>(expr: &'a Expression<'a>) -> Option<&'a str> {
    match expr {
        Expression::Identifier(id) => Some(id.name.as_str()),
        Expression::StaticMemberExpression(m) => root_identifier(&m.object),
        Expression::ComputedMemberExpression(m) => root_identifier(&m.object),
        _ => None,
    }
}

/// True if `text` contains `name` as a standalone JavaScript identifier (i.e. the
/// surrounding characters are not identifier characters), not merely a substring.
/// `pathname` matches in `pathname === x` but not in `mypathname`.
fn references_identifier(text: &str, name: &str) -> bool {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(rel) = memchr::memmem::find(&bytes[from..], name.as_bytes()) {
        let start = from + rel;
        let end = start + name.len();
        let before_ok = start == 0 || !is_ident_char(bytes[start - 1]);
        let after_ok = end >= bytes.len() || !is_ident_char(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        from = start + 1;
    }
    false
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["useEffect"])
    }

    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::CallExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::CallExpression(call) = node.kind() else {
            return;
        };

        // Check callee is `useEffect`.
        let Expression::Identifier(callee_ident) = &call.callee else {
            return;
        };
        if callee_ident.name.as_str() != "useEffect" {
            return;
        }

        // First argument must be an arrow function.
        let Some(first_arg) = call.arguments.first() else {
            return;
        };
        let oxc_ast::ast::Argument::ArrowFunctionExpression(arrow) = first_arg else {
            return;
        };

        // Body must have exactly one statement.
        let body = &arrow.body.statements;
        if body.len() != 1 {
            return;
        }
        let Statement::ExpressionStatement(expr_stmt) = &body[0] else {
            return;
        };
        let Expression::CallExpression(inner_call) = &expr_stmt.expression else {
            return;
        };

        // Check for side-effect patterns: await, fetch(), subscribe(), addEventListener()
        let inner_start = inner_call.span.start as usize;
        let inner_end = inner_call.span.end as usize;
        if inner_end <= ctx.source.len() {
            let call_text = &ctx.source[inner_start..inner_end];
            if call_text.contains("await")
                || call_text.contains("fetch(")
                || call_text.contains("subscribe(")
                || call_text.contains("addEventListener(")
            {
                return;
            }
        }

        // Check that the inner call is a setter (starts with "set").
        let inner_name = match &inner_call.callee {
            Expression::Identifier(id) => id.name.as_str(),
            _ => return,
        };
        if !inner_name.starts_with("set") {
            return;
        }

        // Only flag when the setter argument is DERIVED — it references one of the
        // effect's dependencies. `setIsOpen(false)` resets state to a constant in
        // response to a dependency change (a side-effect), not a derivation.
        let Some(deps_expr) = call.arguments.get(1).and_then(|a| a.as_expression()) else {
            return;
        };
        let Expression::ArrayExpression(deps_arr) = deps_expr else {
            return;
        };
        let dep_roots = dependency_roots(deps_arr);
        if dep_roots.is_empty() {
            return;
        }
        let Some(setter_arg) = inner_call.arguments.first().and_then(|a| a.as_expression())
        else {
            return;
        };
        let arg_start = setter_arg.span().start as usize;
        let arg_end = setter_arg.span().end as usize;
        if arg_end > ctx.source.len() {
            return;
        }
        let arg_text = &ctx.source[arg_start..arg_end];
        if !dep_roots
            .iter()
            .any(|dep| references_identifier(arg_text, dep))
        {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Derived state in `useEffect` is an anti-pattern. Compute the value during render instead.".into(),
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.tsx")
    }

    #[test]
    fn flags_derived_value_from_dependency() {
        assert_eq!(
            run("useEffect(() => { setFull(first + ' ' + last) }, [first, last])").len(),
            1
        );
    }

    // Regression for #2215: `setIsOpen(false)` resets state to a literal constant
    // in response to a navigation change — a side-effect, not derived state.
    #[test]
    fn allows_setter_with_literal_constant() {
        assert!(
            run("useEffect(() => { setIsOpen(false) }, [pathname, searchParams])").is_empty()
        );
    }

    // Guard for #2215: comparing a dependency still computes a value FROM the
    // dependency, so it remains derived state and must flag.
    #[test]
    fn flags_setter_referencing_dependency() {
        assert_eq!(
            run("useEffect(() => { setActive(pathname === item.path) }, [pathname])").len(),
            1
        );
    }
}
