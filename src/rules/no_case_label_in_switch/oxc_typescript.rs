use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::LabeledStatement]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::LabeledStatement(labeled) = node.kind() else {
            return;
        };

        // Check if any ancestor is a SwitchStatement.
        let inside_switch = semantic
            .nodes()
            .ancestors(node.id())
            .any(|a| matches!(a.kind(), AstKind::SwitchStatement(_)));

        if !inside_switch {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, labeled.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Label inside switch statement \u{2014} this is a JS label, not a case branch. Use `case <value>:` instead.".into(),
            severity: Severity::Error,
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
    fn flags_label_in_switch() {
        let src = r#"
switch (action) {
    case "run":
        break;
    stop:
        console.log("stopped");
        break;
}
"#;
        assert_eq!(run_on(src).len(), 1);
    }


    #[test]
    fn flags_multiple_labels() {
        let src = r#"
switch (x) {
    case 1:
        break;
    foo:
        break;
    bar:
        break;
}
"#;
        assert_eq!(run_on(src).len(), 2);
    }


    #[test]
    fn allows_case_and_default() {
        let src = r#"
switch (x) {
    case "a":
        break;
    case "b":
        break;
    default:
        break;
}
"#;
        assert!(run_on(src).is_empty());
    }


    #[test]
    fn ignores_labels_outside_switch() {
        let src = r#"
myLabel:
for (let i = 0; i < 10; i++) {
    break myLabel;
}
"#;
        assert!(run_on(src).is_empty());
    }
}
