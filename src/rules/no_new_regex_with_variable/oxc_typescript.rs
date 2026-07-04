//! no-new-regex-with-variable oxc backend — flag `new RegExp(variable)`.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{Argument, BinaryOperator, Expression, VariableDeclarationKind};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::NewExpression]
    }

    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["RegExp"])
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::NewExpression(new_expr) = node.kind() else { return };

        let Expression::Identifier(ctor) = &new_expr.callee else { return };
        if ctor.name.as_str() != "RegExp" {
            return;
        }

        let Some(first_arg) = new_expr.arguments.first() else { return };
        // A string/template literal is a fixed pattern. So is `RE.source`: `.source`
        // yields a regex's pattern text, so `new RegExp(RE.source, flags)` (rebuilding
        // a regex with different flags) carries no variable input. This rule has no
        // symbol resolution, so it accepts any `.source` access rather than resolving
        // the object to a const regex literal — the security rule
        // `security-detect-non-literal-regexp` enforces that tighter check. A bare
        // variable (`new RegExp(pattern)`) is still flagged — only `.source` is exempt.
        //
        // A `+`-concatenation whose operands are all constant strings (string
        // literals and `const`-bound string literals) is fixed at authoring time —
        // `is_constant_string_expr` recognizes it as constant, so
        // `new RegExp('a' + CONST + 'b')` carries no runtime input and is safe like
        // the bare literal arms above.
        let is_safe_first_arg = match first_arg {
            Argument::StringLiteral(_) | Argument::TemplateLiteral(_) => true,
            Argument::StaticMemberExpression(member) => member.property.name.as_str() == "source",
            _ => false,
        } || first_arg
            .as_expression()
            .is_some_and(|expr| is_constant_string_expr(expr, semantic, &mut Vec::new()));
        if is_safe_first_arg {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, new_expr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`new RegExp(variable)` — ReDoS risk. A crafted \
                      pattern can freeze the event loop via exponential \
                      backtracking. Use a literal regex or a vetted \
                      safe-regex library."
                .into(),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True when `expr` is a compile-time-constant string — its value is fixed at
/// authoring time, so it carries no runtime/attacker input and no ReDoS surface:
/// - a string literal;
/// - a template literal with no `${}` interpolation slots (an interpolated
///   template is not constant in this const-fold context);
/// - a `+` concatenation whose both operands are themselves constant strings;
/// - an identifier bound by a `const` whose initializer is itself a constant
///   string expression (resolved via `reference_id` → symbol → declarator).
///
/// This is the same author-fixed-pattern signal the rule already grants a bare
/// template literal, so a strictly-more-static literal concatenation is safe too.
///
/// `visited` carries the symbols currently being resolved so a cyclic `const`
/// chain (`const a = a`, `const a = b; const b = a`) terminates instead of
/// recursing forever.
fn is_constant_string_expr(
    expr: &Expression,
    semantic: &oxc_semantic::Semantic,
    visited: &mut Vec<oxc_semantic::SymbolId>,
) -> bool {
    match expr {
        Expression::StringLiteral(_) => true,
        Expression::TemplateLiteral(tpl) => tpl.expressions.is_empty(),
        Expression::BinaryExpression(bin) => {
            bin.operator == BinaryOperator::Addition
                && is_constant_string_expr(&bin.left, semantic, visited)
                && is_constant_string_expr(&bin.right, semantic, visited)
        }
        Expression::Identifier(ident) => is_const_string_binding(ident, semantic, visited),
        _ => false,
    }
}

/// True when `ident` resolves to a `const` binding whose initializer is itself a
/// constant string expression. A `let`/`var` binding can be reassigned and a
/// function parameter (or an unresolved global/import) is runtime input, so none
/// qualifies. Resolves the binding via `reference_id` → symbol → declaration node.
fn is_const_string_binding(
    ident: &oxc_ast::ast::IdentifierReference,
    semantic: &oxc_semantic::Semantic,
    visited: &mut Vec<oxc_semantic::SymbolId>,
) -> bool {
    use oxc_ast::AstKind as OxcAstKind;

    let Some(ref_id) = ident.reference_id.get() else {
        return false;
    };
    let scoping = semantic.scoping();
    let Some(sym_id) = scoping.get_reference(ref_id).symbol_id() else {
        return false;
    };
    let decl_node_id = scoping.symbol_declaration(sym_id);
    let nodes = semantic.nodes();
    // Stop the walk at the binding's own declaration boundary — a `FormalParameter`
    // is not a `const`, so it (like any non-`VariableDeclarator`) is rejected rather
    // than climbing to an enclosing declarator.
    let Some(OxcAstKind::VariableDeclarator(decl)) = std::iter::once(nodes.kind(decl_node_id))
        .chain(nodes.ancestor_kinds(decl_node_id))
        .find(|kind| {
            matches!(
                kind,
                OxcAstKind::VariableDeclarator(_) | OxcAstKind::FormalParameter(_)
            )
        })
    else {
        return false;
    };
    if decl.kind != VariableDeclarationKind::Const {
        return false;
    }
    let Some(init) = &decl.init else {
        return false;
    };
    if visited.contains(&sym_id) {
        return false;
    }
    visited.push(sym_id);
    let is_constant = is_constant_string_expr(init, semantic, visited);
    visited.pop();
    is_constant
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
        crate::rules::test_helpers::run_rule(&Check, src, "t.ts")
    }

    #[test]
    fn flags_dynamic_regexp() {
        assert_eq!(run("const r = new RegExp(userInput);").len(), 1);
    }

    #[test]
    fn allows_static_regexp() {
        assert!(run(r#"const r = new RegExp("^foo$");"#).is_empty());
    }

    #[test]
    fn skips_dynamic_regexp_in_test_file() {
        // Regression for rbaumier/comply#6059 — `new RegExp(f.exception)` used as
        // the error-matcher argument to `assert.throws()` in a `.spec.ts` test.
        // The pattern is fixture-derived and never reaches a running service, so
        // there is no ReDoS attack surface. Mirrors the Rust backend (#3287),
        // which already exempts `tests/` and `#[test]` code.
        let src = r#"
            fixtures.invalid.forEach(f => {
              it('throws', () => {
                assert.throws(() => baddress.fromBase58Check(f.address),
                  new RegExp(f.address + ' ' + f.exception));
              });
            });
        "#;
        let diags =
            crate::rules::test_helpers::run_rule_gated(&Check, src, "test/address.spec.ts");
        assert!(diags.is_empty());
    }

    #[test]
    fn still_flags_dynamic_regexp_in_production_source() {
        // The test-dir skip is scoped to test files only — a dynamic regex in
        // production source can still be driven by attacker input and is flagged.
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            "const r = new RegExp(req.query.pattern);",
            "src/router.ts",
        );
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_regex_source_member() {
        // Issue #6282: `new RegExp(RE.source, flags)` rebuilds a regex with different
        // flags from a regex literal — `.source` is the fixed pattern text, not input.
        let src = r#"
            const HEAD_SSR_FILTER_RE = /\bhead\.ssr\b/;
            const HEAD_SSR_RE = new RegExp(HEAD_SSR_FILTER_RE.source, 'g');
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_plain_variable_not_source() {
        // Negative control: a plain variable (not `.source`) is still flagged.
        assert_eq!(run("const r = new RegExp(someStringVar);").len(), 1);
    }

    #[test]
    fn allows_constant_string_concat_of_const_binding() {
        // Issue #7237 (sveltejs/svelte mapped_code.js): every operand of the `+`
        // concatenation is a string literal or `r_in`, a const bound to a string
        // literal — the whole pattern is fixed at authoring time, no ReDoS surface.
        let src = r#"
            const r_in = '[#@]\s*sourceMappingURL\s*=\s*(\S*)';
            const regex = new RegExp('(?://' + r_in + ')|(?:/\*' + r_in + '\s*\*/)$');
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_simple_const_concat() {
        // `'a' + r_in + 'b'` where `r_in` is a const bound to a string literal.
        let src = r#"
            const r_in = 'x';
            const r = new RegExp('a' + r_in + 'b');
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_const_bound_string_identifier() {
        // A bare identifier bound by a const to a string literal is a fixed pattern.
        let src = r#"
            const p = 'static';
            const r = new RegExp(p);
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_literal_only_concat() {
        // A concatenation of pure string literals folds to a fixed pattern.
        assert!(run(r#"const r = new RegExp('^' + 'foo' + '$');"#).is_empty());
    }

    #[test]
    fn allows_bare_template_literal() {
        // A bare template literal stays safe (existing behavior).
        assert!(run("const r = new RegExp(`^tpl$`);").is_empty());
    }

    #[test]
    fn flags_param_binding() {
        // A function parameter is runtime input — still flagged.
        let src = r#"
            function build(userInput: string): RegExp {
                return new RegExp(userInput);
            }
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_concat_with_let_operand() {
        // `q` is a `let` bound to a runtime call — a non-const operand keeps the
        // concatenation dynamic, so it stays flagged.
        let src = r#"
            let q = getQuery();
            const r = new RegExp('^' + q + '$');
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_repeated_const_operand() {
        // The same const used twice in one concatenation stays constant — the
        // cycle-guard backtracks per resolution branch, it does not over-reject.
        let src = r#"
            const p = 'x';
            const r = new RegExp(p + p);
        "#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_self_referential_const() {
        // `const a = a` is a cyclic initializer — resolution must terminate (no
        // stack overflow) and, being non-constant, the pattern stays flagged.
        let src = r#"
            const a = a;
            const r = new RegExp(a);
        "#;
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_mutually_recursive_const() {
        // A two-hop const cycle must also terminate and stay flagged.
        let src = r#"
            const a = b;
            const b = a;
            const r = new RegExp(a);
        "#;
        assert_eq!(run(src).len(), 1);
    }
}
