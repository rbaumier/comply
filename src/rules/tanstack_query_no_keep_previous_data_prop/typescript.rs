//! tanstack-query-no-keep-previous-data-prop backend.
//!
//! Flag `keepPreviousData: true` pairs. v5 replaced this with
//! `placeholderData: keepPreviousData` (the imported helper).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["pair"] => |node, source, ctx, diagnostics|
    let Some(key) = node.child_by_field_name("key") else { return; };
    let Ok(key_text) = key.utf8_text(source) else { return; };
    let key_name = key_text.trim_matches(|c| c == '"' || c == '\'');
    if key_name != "keepPreviousData" { return; }
    let Some(value) = node.child_by_field_name("value") else { return; };
    if value.kind() != "true" { return; }
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "`keepPreviousData: true` was removed in v5 — use `placeholderData: keepPreviousData` instead.".into(),
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
    fn flags() {
        assert_eq!(
            run("useQuery({ queryKey: ['x'], queryFn: f, keepPreviousData: true })").len(),
            1
        );
    }

    #[test]
    fn allows() {
        assert!(
            run("useQuery({ queryKey: ['x'], queryFn: f, placeholderData: keepPreviousData })")
                .is_empty()
        );
    }
}
