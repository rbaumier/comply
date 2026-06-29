use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::{
    AssignmentOperator, AssignmentTarget, BindingPattern, Expression, Statement,
    VariableDeclarationKind,
};
use oxc_span::{GetSpan, Span};
use std::sync::Arc;

pub struct Check;

struct AssignInfo<'a> {
    name: &'a str,
    is_const: bool,
    is_compound: bool,
    start: u32,
    rhs: Span,
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::Program, AstType::BlockStatement, AstType::FunctionBody]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let stmts: Option<&oxc_allocator::Vec<'a, Statement<'a>>> = match node.kind() {
            AstKind::Program(prog) => Some(&prog.body),
            AstKind::BlockStatement(block) => Some(&block.body),
            AstKind::FunctionBody(body) => Some(&body.statements),
            _ => None,
        };
        let Some(stmts) = stmts else { return };
        check_consecutive_assignments(stmts, ctx, semantic, diagnostics);
    }
}

fn check_consecutive_assignments(
    stmts: &[Statement],
    ctx: &CheckCtx,
    semantic: &oxc_semantic::Semantic,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let infos: Vec<Option<AssignInfo>> = stmts.iter().map(|s| extract_assign(s)).collect();
    for pair in infos.windows(2) {
        let (Some(a), Some(b)) = (&pair[0], &pair[1]) else {
            continue;
        };
        if a.is_const || a.name != b.name {
            continue;
        }
        // A read-modify-write is not redundant: a compound op always reads the
        // previous value, and a plain assignment whose RHS references the
        // variable consumes it before overwriting.
        if b.is_compound || rhs_reads_var(semantic, a.name, b.rhs) {
            continue;
        }
        // A `@ts-expect-error` directive in the gap before `b` marks it as an
        // intentional type-error probe: the directive fails the build unless the
        // next line is itself a type error, so each overwrite is a distinct
        // assertion (proving the RHS is unassignable), not a redundant write.
        // Scan from the end of `a`'s RHS so a directive string inside `a` itself
        // cannot mask a genuinely redundant pair.
        if guarded_by_ts_expect_error(ctx.source, a.rhs.end, b.start) {
            continue;
        }
        let (line_a, col_a) = byte_offset_to_line_col(ctx.source, a.start as usize);
        let (line_b, _) = byte_offset_to_line_col(ctx.source, b.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line: line_a,
            column: col_a,
            rule_id: super::META.id.into(),
            message: format!(
                "Variable `{}` is assigned on line {} then immediately overwritten on line {}.",
                a.name, line_a, line_b,
            ),
            severity: super::META.severity,
            span: None,
        });
    }
}

fn extract_assign<'a>(stmt: &'a Statement<'a>) -> Option<AssignInfo<'a>> {
    match stmt {
        Statement::VariableDeclaration(decl) => {
            if decl.declarations.len() != 1 {
                return None;
            }
            let declarator = &decl.declarations[0];
            let BindingPattern::BindingIdentifier(id) = &declarator.id else {
                return None;
            };
            let init = declarator.init.as_ref()?;
            let is_const = decl.kind == VariableDeclarationKind::Const;
            Some(AssignInfo {
                name: id.name.as_str(),
                is_const,
                is_compound: false,
                start: stmt.span().start,
                rhs: init.span(),
            })
        }
        Statement::ExpressionStatement(expr_stmt) => {
            let Expression::AssignmentExpression(assign) = &expr_stmt.expression else {
                return None;
            };
            let AssignmentTarget::AssignmentTargetIdentifier(id) = &assign.left else {
                return None;
            };
            Some(AssignInfo {
                name: id.name.as_str(),
                is_const: false,
                is_compound: assign.operator != AssignmentOperator::Assign,
                start: stmt.span().start,
                rhs: assign.right.span(),
            })
        }
        _ => None,
    }
}

/// True when `name` appears as an identifier reference anywhere inside `rhs`.
fn rhs_reads_var(semantic: &oxc_semantic::Semantic, name: &str, rhs: Span) -> bool {
    semantic.nodes().iter().any(|node| {
        let AstKind::IdentifierReference(id) = node.kind() else {
            return false;
        };
        id.name.as_str() == name && rhs.start <= id.span.start && id.span.end <= rhs.end
    })
}

/// True when a `@ts-expect-error` directive appears in the source slice
/// `[gap_start, gap_end)` — the gap between two consecutive assignments. The
/// slice is fetched with `get` so a non-char-boundary offset returns `None`
/// rather than panicking on multi-byte source.
fn guarded_by_ts_expect_error(source: &str, gap_start: u32, gap_end: u32) -> bool {
    source
        .get(gap_start as usize..gap_end as usize)
        .is_some_and(|gap| gap.contains("@ts-expect-error"))
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_immediate_overwrite() {
        let d = run_on("let x = 1;\nx = 2;");
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`x`"));
    }

    #[test]
    fn flags_reassignment_pair() {
        let d = run_on("x = foo();\nx = bar();");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn allows_different_variables() {
        assert!(run_on("let x = 1;\nlet y = 2;").is_empty());
    }

    #[test]
    fn allows_used_between() {
        assert!(run_on("let x = 1;\nconsole.log(x);\nx = 2;").is_empty());
    }

    #[test]
    fn allows_promise_chain() {
        assert!(run_on("let chain = glob(p);\nchain = chain.then((r) => r.sort());").is_empty());
    }

    #[test]
    fn allows_compound_assignment() {
        assert!(run_on("let result = \"Object {\";\nresult += printObjectProperties(val);").is_empty());
    }

    #[test]
    fn allows_read_modify_write_via_argument() {
        assert!(run_on("authDef = Buffer.from(authDef).toString();\nauthDef = authDef.split(\":\");").is_empty());
    }

    #[test]
    fn allows_consecutive_ts_expect_error_probes() {
        let src = "declare let foo: EmptyObject;\n// @ts-expect-error\nfoo = [];\n// @ts-expect-error\nfoo = {x: 1};\n// @ts-expect-error\nfoo = 42;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn skips_only_the_ts_expect_error_guarded_pair() {
        // `a = 1; a = 2;` is unguarded and stays flagged; the `b` pair is guarded.
        let src = "let a = 1;\na = 2;\nlet b = 1;\n// @ts-expect-error\nb = 2;";
        let d = run_on(src);
        assert_eq!(d.len(), 1);
        assert!(d[0].message.contains("`a`"));
    }

    #[test]
    fn flags_redundant_with_plain_comment_between() {
        // A non-directive comment must not suppress a genuinely redundant pair.
        let d = run_on("let x = 1;\n// just a note\nx = 2;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_redundant_when_directive_is_inside_first_rhs() {
        // The directive substring lives inside `a`'s own string literal, not in
        // the gap before `b`, so the redundant pair must still be flagged.
        let d = run_on("let x = \"@ts-expect-error\";\nx = 2;");
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn handles_multibyte_source_in_gap() {
        // Non-ASCII bytes in the scanned gap must not break the byte slice.
        assert!(run_on("let x = 1;\n// café \u{2615} @ts-expect-error\nx = 2;").is_empty());
    }
}
