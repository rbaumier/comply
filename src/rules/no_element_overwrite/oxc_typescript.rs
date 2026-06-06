use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Statement;
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// Extract the assignment target from bracket-notation: `arr[0] = ...` -> `arr[0]`
fn bracket_target(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let bracket_end = trimmed.find(']')?;
    let _bracket_start = trimmed[..bracket_end].find('[')?;
    let after = trimmed[bracket_end + 1..].trim_start();
    if after.starts_with('=') && !after.starts_with("==") {
        Some(trimmed[..bracket_end + 1].to_string())
    } else {
        None
    }
}

/// Extract the key from `.set("key", ...)` -> `<receiver>.set(<key>)`
fn map_set_target(text: &str) -> Option<String> {
    let trimmed = text.trim();
    let pos = trimmed.find(".set(")?;
    let receiver = trimmed[..pos].trim();
    let args_start = pos + 5;
    let rest = &trimmed[args_start..];
    let comma = rest.find(',')?;
    let key = rest[..comma].trim();
    Some(format!("{}.set({})", receiver, key))
}

fn stmt_text<'a>(stmt: &Statement, source: &'a str) -> &'a str {
    let span = stmt.span();
    &source[span.start as usize..span.end as usize]
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[
            AstType::Program,
            AstType::BlockStatement,
            AstType::FunctionBody,
        ]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
            let stmts: Option<&oxc_allocator::Vec<'a, Statement<'a>>> = match node.kind() {
                AstKind::Program(prog) => Some(&prog.body),
                AstKind::BlockStatement(block) => Some(&block.body),
                AstKind::FunctionBody(body) => Some(&body.statements),
                _ => None,
            };
            let Some(stmts) = stmts else { return };

            for pair in stmts.windows(2) {
                let (s1, s2) = (&pair[0], &pair[1]);
                if !matches!(s1, Statement::ExpressionStatement(_))
                    || !matches!(s2, Statement::ExpressionStatement(_))
                {
                    continue;
                }
                let text1 = stmt_text(s1, ctx.source);
                let text2 = stmt_text(s2, ctx.source);

                // Check bracket notation.
                if let (Some(t1), Some(t2)) = (bracket_target(text1), bracket_target(text2))
                    && t1 == t2 {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, s2.span().start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`{}` is assigned on the previous line and immediately overwritten.",
                                t1
                            ),
                            severity: super::META.severity,
                            span: None,
                        });
                        continue;
                    }

                // Check .set() calls.
                if let (Some(t1), Some(t2)) = (map_set_target(text1), map_set_target(text2))
                    && t1 == t2 {
                        let (line, column) =
                            byte_offset_to_line_col(ctx.source, s2.span().start as usize);
                        diagnostics.push(Diagnostic {
                            path: Arc::clone(&ctx.path_arc),
                            line,
                            column,
                            rule_id: super::META.id.into(),
                            message: "`.set()` with the same key on the previous line — first write is dead.".into(),
                            severity: super::META.severity,
                            span: None,
                        });
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
    fn flags_consecutive_bracket_writes() {
        let src = "arr[0] = 1;\narr[0] = 2;";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn flags_consecutive_map_set() {
        let src = "map.set(\"key\", 1);\nmap.set(\"key\", 2);";
        assert_eq!(run_on(src).len(), 1);
    }

    #[test]
    fn allows_different_indices() {
        let src = "arr[0] = 1;\narr[1] = 2;";
        assert!(run_on(src).is_empty());
    }

    #[test]
    fn allows_different_keys() {
        let src = "map.set(\"a\", 1);\nmap.set(\"b\", 2);";
        assert!(run_on(src).is_empty());
    }
}
