use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["call_expression"] => |node, source, ctx, diagnostics|
    let Some(callee) = node.child_by_field_name("function") else { return; };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(prop) = callee.child_by_field_name("property") else { return; };
    if prop.utf8_text(source).unwrap_or("") != "matchErrorPartial" {
        return;
    }

    // Without type info we can't know whether the union is fully enumerated.
    // Conservative heuristic: only flag when the match object enumerates
    // 3+ tags, suggesting the developer has covered most/all cases.
    let min_tags = ctx.config.threshold("better-result-prefer-matcherror-exhaustive", "min_tags", ctx.lang);
    let Some(args) = node.child_by_field_name("arguments") else { return; };
    let mut cursor = args.walk();
    let Some(obj) = args.children(&mut cursor).find(|c| c.kind() == "object") else { return; };
    let mut ocursor = obj.walk();
    let tag_count = obj
        .children(&mut ocursor)
        .filter(|c| c.kind() == "pair" || c.kind() == "method_definition")
        .count();
    if tag_count < min_tags {
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
    fn flags_match_error_partial_with_three_tags() {
        let src = "result.matchErrorPartial({ NotFound: () => 0, NetworkError: () => 1, ParseError: () => 2 });";
        assert_eq!(run(src).len(), 1);
    }
    #[test]
    fn allows_match_error_partial_with_one_tag() {
        let src = "result.matchErrorPartial({ NotFound: () => 0 });";
        assert!(run(src).is_empty());
    }
    #[test]
    fn allows_match_error_partial_with_two_tags() {
        let src = "result.matchErrorPartial({ NotFound: () => 0, NetworkError: () => 1 });";
        assert!(run(src).is_empty());
    }
    #[test]
    fn allows_match_error() {
        let src = "result.matchError({ NotFound: () => 0, NetworkError: () => 1 });";
        assert!(run(src).is_empty());
    }
}
