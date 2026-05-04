use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

struct CommentInfo {
    start_line: usize,
    start_col: usize,
    end_line: usize,
    text: String,
}

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[]
    }

    fn run_on_semantic<'a>(
        &self,
        semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        let comments = semantic.comments();
        if comments.is_empty() {
            return Vec::new();
        }

        let mut infos: Vec<CommentInfo> = Vec::with_capacity(comments.len());
        for c in comments {
            let raw = &ctx.source[c.span.start as usize..c.span.end as usize];
            let (start_line, start_col) = byte_offset_to_line_col(ctx.source, c.span.start as usize);
            let (end_line, _) = byte_offset_to_line_col(ctx.source, c.span.end.saturating_sub(1) as usize);
            infos.push(CommentInfo {
                start_line,
                start_col,
                end_line,
                text: raw.to_string(),
            });
        }

        let groups = group_adjacent(&infos);
        let mut diagnostics = Vec::new();

        for group in groups {
            let Some(body) = build_group_body(&group) else {
                continue;
            };
            if !super::has_code_shape(&body) {
                continue;
            }
            if !parses_as_code(&body) {
                continue;
            }
            let first = group[0];
            diagnostics.push(Diagnostic {
                path: Arc::clone(&ctx.path_arc),
                line: first.start_line,
                column: first.start_col,
                rule_id: "no-commented-out-code".into(),
                message: "This comment looks like commented-out code — \
                          delete it. Git history preserves the original."
                    .into(),
                severity: Severity::Warning,
                span: None,
            });
        }

        diagnostics
    }
}

fn group_adjacent(comments: &[CommentInfo]) -> Vec<Vec<&CommentInfo>> {
    let mut groups: Vec<Vec<&CommentInfo>> = Vec::new();
    for c in comments {
        let extend = groups
            .last()
            .and_then(|g| g.last())
            .is_some_and(|last| c.start_line <= last.end_line + 1);
        if extend {
            groups.last_mut().unwrap().push(c);
        } else {
            groups.push(vec![c]);
        }
    }
    groups
}

fn build_group_body(group: &[&CommentInfo]) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    for c in group {
        let Some(stripped) = super::strip_comment_syntax(&c.text) else {
            continue;
        };
        if !stripped.trim().is_empty() {
            lines.push(stripped);
        }
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn parses_as_code(body: &str) -> bool {
    let allocator = oxc_allocator::Allocator::default();
    let source_type = oxc_span::SourceType::ts();
    let ret = oxc_parser::Parser::new(&allocator, body, source_type).parse();
    if !ret.errors.is_empty() {
        return false;
    }
    contains_rich_code(&ret.program)
}

fn contains_rich_code(program: &oxc_ast::ast::Program) -> bool {
    use oxc_ast::ast::{Expression, Statement};
    for stmt in &program.body {
        match stmt {
            Statement::VariableDeclaration(_)
            | Statement::FunctionDeclaration(_)
            | Statement::ClassDeclaration(_)
            | Statement::IfStatement(_)
            | Statement::ForStatement(_)
            | Statement::ForInStatement(_)
            | Statement::WhileStatement(_)
            | Statement::DoWhileStatement(_)
            | Statement::ReturnStatement(_)
            | Statement::ThrowStatement(_)
            | Statement::TryStatement(_)
            | Statement::SwitchStatement(_)
            | Statement::TSTypeAliasDeclaration(_)
            | Statement::TSInterfaceDeclaration(_)
            | Statement::TSEnumDeclaration(_)
            | Statement::ImportDeclaration(_)
            | Statement::ExportDefaultDeclaration(_)
            | Statement::ExportNamedDeclaration(_) => return true,
            Statement::ExpressionStatement(expr_stmt) => match &expr_stmt.expression {
                Expression::CallExpression(_)
                | Expression::AssignmentExpression(_)
                | Expression::NewExpression(_)
                | Expression::UpdateExpression(_)
                | Expression::AwaitExpression(_) => return true,
                _ => {}
            },
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_commented_const() {
        assert_eq!(run("// const x = 5;").len(), 1);
    }

    #[test]
    fn flags_commented_function_call() {
        assert_eq!(run("// foo(bar);").len(), 1);
    }

    #[test]
    fn flags_adjacent_commented_lines() {
        let src = "// const x = 5;\n// const y = 10;";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_prose_comment() {
        assert!(run("// This function computes the total cost.").is_empty());
    }

    #[test]
    fn allows_triple_slash_doc_comment() {
        assert!(run("/// Returns the parsed result.").is_empty());
    }

    #[test]
    fn allows_short_label_comment() {
        assert!(run("// setup").is_empty());
    }

    #[test]
    fn allows_pattern_list_prose() {
        assert!(run("// const foo =, let foo =, var foo =").is_empty());
    }

    #[test]
    fn allows_inline_syntax_description() {
        assert!(run("// const foo =").is_empty());
    }

    #[test]
    fn flags_commented_block_comment() {
        assert_eq!(run("/* const x = 5; foo(x); */").len(), 1);
    }

    #[test]
    fn allows_block_comment_prose() {
        assert!(run("/* this explains what follows */").is_empty());
    }

    #[test]
    fn allows_jsdoc_block_comment() {
        assert!(run("/** @returns the cost */").is_empty());
    }

    #[test]
    fn non_adjacent_comments_produce_separate_diagnostics() {
        let src = "// const x = 5;\nconst y = 10;\n// foo(y);";
        assert_eq!(run(src).len(), 2);
    }
}
