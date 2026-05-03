use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
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
        let Expression::Identifier(id) = &call.callee else {
            return;
        };
        if id.name.as_str() != "uuid" {
            return;
        }
        let chain_src = &ctx.source[call.span.start as usize..];
        let end = chain_src.find(';').unwrap_or(chain_src.len());
        let chain = &chain_src[..end];
        let has_pk = chain.contains(".primaryKey(");
        if !has_pk {
            return;
        }
        let has_v4_default = chain.contains(".defaultRandom(") || chain.contains(".default(");
        if !has_v4_default {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, call.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "UUIDv4 primary key fragments B-tree indexes — use \
                      UUIDv7 or `BIGINT GENERATED ALWAYS AS IDENTITY`."
                .into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(src: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(src, &Check)
    }

    #[test]
    fn flags_uuid_pk_default_random() {
        assert_eq!(run_on("const id = uuid('id').primaryKey().defaultRandom();").len(), 1);
    }

    #[test]
    fn flags_uuid_pk_default_sql() {
        assert_eq!(run_on("const id = uuid('id').primaryKey().default(sql`gen_random_uuid()`);").len(), 1);
    }

    #[test]
    fn allows_uuid_pk_without_default() {
        assert!(run_on("const id = uuid('id').primaryKey();").is_empty());
    }

    #[test]
    fn allows_uuid_default_without_pk() {
        assert!(run_on("const ref_id = uuid('ref_id').defaultRandom();").is_empty());
    }
}
