use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use oxc_ast::ast::{Expression, Statement, VariableDeclarationKind};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

/// If the statement is `const/let x = obj.prop;`, return the object name.
fn extract_object_name<'a>(stmt: &'a Statement<'a>, source: &'a str) -> Option<&'a str> {
    let Statement::VariableDeclaration(decl) = stmt else { return None };
    if !matches!(decl.kind, VariableDeclarationKind::Const | VariableDeclarationKind::Let) {
        return None;
    }
    if decl.declarations.len() != 1 {
        return None;
    }
    let declarator = &decl.declarations[0];
    let Some(ref init) = declarator.init else { return None };
    let Expression::StaticMemberExpression(member) = init else { return None };
    let Expression::Identifier(obj) = &member.object else { return None };
    let name = obj.name.as_str();
    if name.is_empty() { return None; }
    Some(&source[obj.span.start as usize..obj.span.end as usize])
}

fn scan_statements<'a>(stmts: &'a [Statement<'a>], source: &'a str, ctx: &CheckCtx, diagnostics: &mut Vec<Diagnostic>) {
    let mut i = 0;
    while i < stmts.len() {
        if let Some(obj_name) = extract_object_name(&stmts[i], source) {
            let start = i;
            let mut count = 1usize;
            let mut j = i + 1;
            while j < stmts.len() {
                if let Some(next_obj) = extract_object_name(&stmts[j], source)
                    && next_obj == obj_name {
                        count += 1;
                        j += 1;
                        continue;
                    }
                break;
            }
            if count >= 2 {
                let span = stmts[start].span();
                let (line, column) = byte_offset_to_line_col(source, span.start as usize);
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: format!(
                        "{count} consecutive property accesses on `{obj_name}` — use destructuring instead."
                    ),
                    severity: Severity::Warning,
                    span: None,
                });
                i = j;
                continue;
            }
        }

        // Recurse into blocks
        if let Statement::BlockStatement(block) = &stmts[i] {
            scan_statements(&block.body, source, ctx, diagnostics);
        }

        i += 1;
    }
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
        let program = semantic.nodes().program();
        scan_statements(&program.body, ctx.source, ctx, &mut diagnostics);
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }


    #[test]
    fn flags_consecutive_accesses() {
        let src = "const x = obj.x;\nconst y = obj.y;";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("obj"));
    }


    #[test]
    fn flags_three_consecutive() {
        let src = "const a = config.a;\nconst b = config.b;\nconst c = config.c;";
        let diags = run_on(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("3"));
    }


    #[test]
    fn allows_single_access() {
        assert!(run_on("const x = obj.x;").is_empty());
    }


    #[test]
    fn allows_different_objects() {
        let src = "const x = obj1.x;\nconst y = obj2.y;";
        assert!(run_on(src).is_empty());
    }
}
