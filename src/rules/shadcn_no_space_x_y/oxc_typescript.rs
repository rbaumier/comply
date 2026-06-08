use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

fn is_space_class(class: &str) -> bool {
    let utility = class.rsplit(':').next().unwrap_or(class);
    let utility = utility.trim_start_matches('!');
    utility.starts_with("space-x-") || utility.starts_with("space-y-")
}

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::JSXAttribute]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::JSXAttribute(attr) = node.kind() else {
            return;
        };
        let oxc_ast::ast::JSXAttributeName::Identifier(ident) = &attr.name else {
            return;
        };
        if ident.name.as_str() != "className" {
            return;
        }
        let Some(oxc_ast::ast::JSXAttributeValue::StringLiteral(lit)) = &attr.value else {
            return;
        };
        let class_str = lit.value.as_str();
        if !class_str.split_ascii_whitespace().any(is_space_class) {
            return;
        }
        let (line, column) =
            byte_offset_to_line_col(ctx.source, attr.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "`space-x-*` / `space-y-*` are fragile — use `flex gap-*` (or `flex flex-col gap-*`) instead.".into(),
            severity: Severity::Warning,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_tsx(s, &Check)
    }


    #[test]
    fn flags_space_x() {
        assert_eq!(
            run(r#"const x = <div className="space-x-2">x</div>;"#).len(),
            1
        );
    }


    #[test]
    fn flags_space_y_with_other_classes() {
        assert_eq!(
            run(r#"const x = <div className="p-4 space-y-4 items-start">x</div>;"#).len(),
            1
        );
    }


    #[test]
    fn allows_flex_gap() {
        assert!(run(r#"const x = <div className="flex gap-2">x</div>;"#).is_empty());
    }


    #[test]
    fn allows_no_classname() {
        assert!(run(r#"const x = <div>x</div>;"#).is_empty());
    }
}
