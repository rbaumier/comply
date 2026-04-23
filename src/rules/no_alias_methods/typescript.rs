//! no-alias-methods backend — flag Jest/Vitest alias matchers used in
//! `expect(...).aliasMatcher(...)` chains and suggest the canonical form.

use crate::diagnostic::{Diagnostic, Severity};

/// (alias, canonical) pairs for Jest/Vitest matchers.
const ALIASES: &[(&str, &str)] = &[
    ("toBeCalled", "toHaveBeenCalled"),
    ("toBeCalledTimes", "toHaveBeenCalledTimes"),
    ("toBeCalledWith", "toHaveBeenCalledWith"),
    ("lastCalledWith", "toHaveBeenLastCalledWith"),
    ("nthCalledWith", "toHaveBeenNthCalledWith"),
    ("toReturn", "toHaveReturned"),
    ("toReturnTimes", "toHaveReturnedTimes"),
    ("toReturnWith", "toHaveReturnedWith"),
    ("lastReturnedWith", "toHaveLastReturnedWith"),
    ("nthReturnedWith", "toHaveNthReturnedWith"),
    ("toThrowError", "toThrow"),
];

fn canonical_for(alias: &str) -> Option<&'static str> {
    ALIASES
        .iter()
        .find(|(a, _)| *a == alias)
        .map(|(_, canonical)| *canonical)
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "call_expression" {
        return;
    }
    let Some(callee) = node.child_by_field_name("function") else { return };
    if callee.kind() != "member_expression" {
        return;
    }
    let Some(property) = callee.child_by_field_name("property") else { return };
    let Ok(property_name) = property.utf8_text(source) else { return };
    let Some(canonical) = canonical_for(property_name) else { return };
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &property,
        "no-alias-methods",
        format!("`{property_name}` is an alias for `{canonical}` — use the canonical matcher name."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_to_be_called() {
        assert_eq!(run_on("expect(fn).toBeCalled()").len(), 1);
    }

    #[test]
    fn flags_to_be_called_with() {
        assert_eq!(run_on("expect(fn).toBeCalledWith(1, 2)").len(), 1);
    }

    #[test]
    fn flags_last_called_with() {
        assert_eq!(run_on("expect(fn).lastCalledWith('a')").len(), 1);
    }

    #[test]
    fn flags_to_throw_error() {
        assert_eq!(run_on("expect(fn).toThrowError('boom')").len(), 1);
    }

    #[test]
    fn flags_nth_returned_with() {
        assert_eq!(run_on("expect(fn).nthReturnedWith(1, 'x')").len(), 1);
    }

    #[test]
    fn allows_canonical_to_have_been_called() {
        assert!(run_on("expect(fn).toHaveBeenCalled()").is_empty());
    }

    #[test]
    fn allows_canonical_to_throw() {
        assert!(run_on("expect(fn).toThrow('boom')").is_empty());
    }

    #[test]
    fn allows_unrelated_method() {
        assert!(run_on("arr.map(x => x)").is_empty());
    }
}
