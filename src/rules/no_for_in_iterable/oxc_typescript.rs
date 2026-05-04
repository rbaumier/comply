use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

const ITERABLE_HINTS: &[&str] = &[
    "arr", "list", "items", "elements", "array", "values", "entries", "results", "rows", "records",
];

fn looks_like_iterable(rhs: &str) -> bool {
    if rhs.starts_with('[') {
        return true;
    }
    let rhs_lower = rhs.to_ascii_lowercase();
    ITERABLE_HINTS.iter().any(|hint| rhs_lower.contains(hint))
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ForInStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ForInStatement(stmt) = node.kind() else {
            return;
        };
        let rhs_text =
            &ctx.source[stmt.right.span().start as usize..stmt.right.span().end as usize];
        if !looks_like_iterable(rhs_text) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, stmt.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`for...in` on an array/iterable — use `for...of` instead.".into(),
            severity: super::META.severity,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_for_in_with_array_name() {
        assert_eq!(run_on("for (const x in myArray) {}").len(), 1);
    }

    #[test]
    fn flags_for_in_with_list_name() {
        assert_eq!(run_on("for (let key in itemsList) {}").len(), 1);
    }

    #[test]
    fn allows_for_in_with_object() {
        assert!(run_on("for (const key in obj) {}").is_empty());
    }

    #[test]
    fn allows_for_of() {
        assert!(run_on("for (const x of myArray) {}").is_empty());
    }
}
