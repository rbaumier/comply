use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return; };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return; };
    if prop.utf8_text(source).unwrap_or("") != "matchErrorPartial" {
        return;
    }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Prefer matchError (exhaustive) over matchErrorPartial when the union is fully enumerable.".into(),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(s, &Check)
    }
    #[test]
    fn flags_match_error_partial() {
        let src = "result.matchErrorPartial({ NotFound: () => 0 });";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_match_error() {
        let src = "result.matchError({ NotFound: () => 0, NetworkError: () => 1 });";
        assert!(run(src).is_empty());
    }
}
