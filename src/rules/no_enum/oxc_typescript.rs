use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TSEnumDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::TSEnumDeclaration(decl) = node.kind() else {
            return;
        };
        let name = decl.id.name.as_str();
        let (line, column) = byte_offset_to_line_col(ctx.source, decl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "Enum '{name}' — replace with `const {name} = {{ ... }} as const \
                 satisfies Record<string, string>` (for config) or a \
                 discriminated union with a `type` field (for tagged data). \
                 Enums emit runtime code and don't narrow cleanly."
            ),
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
    fn flags_enum_declaration() {
        assert_eq!(run_on("enum Status { Active, Inactive }").len(), 1);
    }

    #[test]
    fn flags_const_enum() {
        assert_eq!(run_on("const enum Role { Admin, User }").len(), 1);
    }

    #[test]
    fn allows_as_const_satisfies() {
        let source = "const STATUS = { active: 'active', inactive: 'inactive' } as const satisfies Record<string, string>;";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn allows_discriminated_union() {
        let source = "type Status = { type: 'active' } | { type: 'inactive' };";
        assert!(run_on(source).is_empty());
    }
}
