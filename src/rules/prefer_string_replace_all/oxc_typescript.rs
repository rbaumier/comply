//! prefer-string-replace-all OXC backend — flag `.replace(/pattern/g, ...)`.

use std::sync::Arc;

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, Expression, RegExpFlags};

pub struct Check;

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&[".replace"])
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
        let AstKind::CallExpression(call) = node.kind() else { return };

        let Expression::StaticMemberExpression(member) = &call.callee else { return };
        if member.property.name.as_str() != "replace" {
            return;
        }

        // First argument must be a regex literal with the `g` flag.
        let Some(first_arg) = call.arguments.first() else { return };
        let Argument::RegExpLiteral(regex) = first_arg else { return };

        if !regex.regex.flags.contains(RegExpFlags::G) {
            return;
        }

        // Anchor at the `replace` property identifier. For a chained member call
        // (`s.replace(/a/g).replace(/b/g)`), oxc spans every `CallExpression` from
        // the chain root, so `call.span.start` would stack all diagnostics on the
        // leftmost object; `member.property.span.start` points at each `.replace`.
        let (line, column) =
            byte_offset_to_line_col(ctx.source, member.property.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Prefer `String#replaceAll()` over `String#replace()` with a global regex."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_by_id("prefer-string-replace-all", source, "t.ts")
    }

    #[test]
    fn flags_replace_with_global_regex() {
        let d = run(r#"str.replace(/foo/g, 'bar')"#);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "prefer-string-replace-all");
        // Anchored at `replace` (column 5), not the `str` chain root (column 1).
        assert_eq!((d[0].line, d[0].column), (1, 5));
    }

    #[test]
    fn allows_replace_without_global() {
        assert!(run(r#"str.replace(/foo/, 'bar')"#).is_empty());
    }

    #[test]
    fn allows_replace_with_string_arg() {
        assert!(run(r#"str.replace('foo', 'bar')"#).is_empty());
    }

    #[test]
    fn allows_replace_all_already() {
        assert!(run(r#"str.replaceAll('foo', 'bar')"#).is_empty());
    }

    // Regression for #3818: a chained `.replace().replace()` must emit one
    // diagnostic per `.replace`, each anchored at its own `replace` method, not
    // all stacked on the chain-root identifier. oxc spans every CallExpression
    // in the chain from the leftmost object, so anchoring at `call.span.start`
    // collapsed every link onto the same column.
    #[test]
    fn chained_replace_anchors_each_link_at_its_own_method() {
        let source = "export function f(s: string) {\n  return s.replace(/#/g, \"%23\").replace(/\\?/g, \"%3F\");\n}";
        let d = run(source);
        assert_eq!(d.len(), 2, "one diagnostic per global-regex .replace");

        // Both links are on line 2; the chain root `s` is at column 10.
        assert_eq!(d[0].line, 2);
        assert_eq!(d[1].line, 2);

        // The two `replace` methods sit at distinct columns: `  return s.` is
        // 11 chars so the first `replace` starts at column 12; the second follows
        // `.replace(/#/g, "%23").` and starts at column 33. (Emission order follows
        // AST traversal — outer call first — so compare as a sorted set.)
        let mut columns: Vec<usize> = d.iter().map(|diag| diag.column).collect();
        columns.sort_unstable();
        assert_eq!(columns, vec![12, 33]);

        // Neither diagnostic is anchored at the chain-root token `s` (column 10).
        assert!(columns.iter().all(|&c| c != 10));
    }
}
