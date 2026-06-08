//! no-identical-title OXC backend — flag repeated describe/test/it titles
//! within the same lexical scope.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::*;
use std::collections::HashSet;
use std::sync::Arc;

pub struct Check;

const TEST_BASES: &[&str] = &["describe", "test", "it"];

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for node in semantic.nodes().iter() {
            if let AstKind::Program(program) = node.kind() {
                check_statements(&program.body, ctx, &mut diagnostics);
            }
        }
        diagnostics
    }
}

/// Extract the base test construct name from a call expression callee.
/// Returns the base kind for `describe`, `test`, `it` (including `.only`/`.skip` variants).
fn classify_callee(expr: &Expression) -> Option<&'static str> {
    match expr {
        Expression::Identifier(id) => {
            TEST_BASES.iter().copied().find(|b| *b == id.name.as_str())
        }
        Expression::StaticMemberExpression(member) => {
            if let Expression::Identifier(obj) = &member.object {
                TEST_BASES.iter().copied().find(|b| *b == obj.name.as_str())
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Extract a static string title from the first argument of a call.
fn static_title(args: &[Argument]) -> Option<String> {
    let first = args.first()?;
    match first {
        Argument::StringLiteral(s) => Some(s.value.to_string()),
        Argument::TemplateLiteral(t) => {
            if !t.expressions.is_empty() {
                return None;
            }
            let mut out = String::new();
            for quasi in &t.quasis {
                out.push_str(quasi.value.raw.as_str());
            }
            Some(out)
        }
        _ => None,
    }
}

/// Walk the direct statement children of a scope, tracking test titles by
/// construct kind. Recurse into describe callback bodies.
fn check_statements(
    stmts: &[Statement],
    ctx: &CheckCtx,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen: HashSet<(&'static str, String)> = HashSet::new();

    for stmt in stmts {
        let Statement::ExpressionStatement(expr_stmt) = stmt else {
            continue;
        };
        let Expression::CallExpression(call) = &expr_stmt.expression else {
            continue;
        };

        let Some(kind) = classify_callee(&call.callee) else {
            continue;
        };
        let Some(title) = static_title(&call.arguments) else {
            continue;
        };

        let key = (kind, title.clone());
        if !seen.insert(key) {
            let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line,
                column,
                rule_id: "no-identical-title".into(),
                message: format!(
                    "Duplicate {kind} title {title:?} in the same scope — use a unique title."
                ),
                severity: Severity::Warning,
                span: None,
            });
        }

        // For describe blocks, recurse into the callback body.
        if kind == "describe"
            && let Some(last_arg) = call.arguments.last() {
                let cb = match last_arg {
                    Argument::ArrowFunctionExpression(f) => Some(&f.body),
                    Argument::FunctionExpression(f) => f.body.as_ref(),
                    _ => None,
                };
                if let Some(body) = cb {
                    check_statements(&body.statements, ctx, diagnostics);
                }
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_duplicate_describe_titles() {
        let src = "\
describe('auth', () => {});
describe('auth', () => {});";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].rule_id, "no-identical-title");
    }


    #[test]
    fn flags_duplicate_test_titles_in_same_describe() {
        let src = "\
describe('auth', () => {
  test('rejects empty', () => {});
  test('rejects empty', () => {});
});";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_duplicate_it_titles() {
        let src = "\
it('works', () => {});
it('works', () => {});";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn flags_only_and_skip_variants_as_same_title() {
        let src = "\
describe('x', () => {});
describe.only('x', () => {});";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
    }


    #[test]
    fn allows_same_title_in_different_describes() {
        let src = "\
describe('a', () => {
  test('handles empty', () => {});
});
describe('b', () => {
  test('handles empty', () => {});
});";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_nested_duplicate_vs_outer() {
        let src = "\
describe('outer', () => {
  test('shared', () => {});
  describe('inner', () => {
    test('shared', () => {});
  });
});";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_distinct_titles() {
        let src = "\
describe('auth', () => {
  test('a', () => {});
  test('b', () => {});
});";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn allows_same_title_on_different_constructs() {
        // describe('x') and test('x') don't collide — different suite/test scopes.
        let src = "\
describe('x', () => {
  test('x', () => {});
});";
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_dynamic_titles() {
        let src = "\
const name = 'x';
test(`case ${name}`, () => {});
test(`case ${name}`, () => {});";
        assert!(run_on(src).is_empty());
    }
}
