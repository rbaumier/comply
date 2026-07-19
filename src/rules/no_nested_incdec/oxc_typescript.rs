//! no-nested-incdec OXC backend — flag `++`/`--` nested inside a larger
//! expression where its side effect is easy to miss (`arr[i++]`, `f(++x)`,
//! `a + b++`). An update that is the whole statement, the entire initializer of
//! a declarator, or the entire right-hand side of an assignment is the
//! outermost operation and is not flagged.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::UpdateExpression]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::UpdateExpression(update) = node.kind() else {
            return;
        };

        let nodes = semantic.nodes();
        let parent_id = nodes.parent_id(node.id());
        if parent_id == node.id() {
            return;
        }
        let parent_kind = nodes.kind(parent_id);

        // Top-level operation: the update is the whole statement, the entire
        // initializer of a declarator (`const N = ++counter`), or the entire
        // right-hand side of an assignment (`x = ++counter`). In each the
        // mutation is the outermost operation and plainly visible — it is not
        // nested inside a larger expression that obscures it.
        if matches!(
            parent_kind,
            AstKind::ExpressionStatement(_)
                | AstKind::VariableDeclarator(_)
                | AstKind::AssignmentExpression(_)
        ) {
            return;
        }
        // For-loop update clause
        if matches!(parent_kind, AstKind::ForStatement(_)) {
            return;
        }
        // Also allow inside sequence_expression that is itself in a for_statement increment
        if matches!(parent_kind, AstKind::SequenceExpression(_)) {
            let grandparent_id = nodes.parent_id(parent_id);
            if grandparent_id != parent_id
                && matches!(nodes.kind(grandparent_id), AstKind::ForStatement(_))
            {
                return;
            }
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, update.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`++`/`--` inside an expression — separate into its own statement for clarity."
                .into(),
            severity: Severity::Error,
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
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn allows_increment_as_const_initializer() {
        // Regression for rbaumier/comply#5459 — the auto-incrementing-id idiom:
        // each `++cnt` is the entire initializer of a `const`, so the mutation
        // is the outermost operation and plainly visible.
        let src = "let cnt = 0;\nconst A = ++cnt;\nconst B = ++cnt;\nconst C = ++cnt;\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_increment_as_assignment_rhs() {
        let src = "let cnt = 0;\nlet x = 0;\nx = ++cnt;\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_standalone_and_for_update() {
        let src = "let i = 0;\ni++;\nfor (let j = 0; j < 3; j++) {}\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn flags_update_nested_in_binary_initializer() {
        // `++call_id << SHIFT` — the update is an operand of a binary expression,
        // so it is genuinely nested even though the binary is a declarator init.
        let src = "let call_id = 0;\nconst SHIFT = 4;\nconst N = ++call_id << SHIFT;\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_update_in_computed_member_initializer() {
        let src = "let i = 0;\nconst arr = [1, 2, 3];\nconst x = arr[i++];\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_update_as_call_argument() {
        let src = "let n = 0;\nfunction f(_x: number) {}\nf(++n);\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_update_nested_in_assignment_rhs() {
        let src = "let a = 0;\nlet b = 0;\nlet c = 0;\na = b + c++;\n";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_update_in_conditional_branch() {
        let src = "let x = 0;\nlet cond = true;\nconst r = cond ? x++ : 0;\n";
        assert_eq!(run(src).len(), 1);
    }
}
