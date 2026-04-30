use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    // Only check once per file (at program node)
    let path_str = ctx.path.to_string_lossy();

    // Only flag exact global patterns
    let is_forbidden = path_str == "types.ts"
        || path_str == "src/types.ts"
        || path_str == "src/types/index.ts"
        || path_str == "types/index.ts"
        || path_str == "src/shared/types.ts"
        || path_str == "shared/types.ts"
        || path_str.ends_with("/shared/types.ts");

    if !is_forbidden { return; }

    let pos = node.start_position();
    diagnostics.push(Diagnostic {
        path: std::sync::Arc::clone(&ctx.path_arc),
        line: pos.row + 1,
        column: pos.column + 1,
        rule_id: "no-global-types-file".into(),
        message: "Global types file — colocate types with the code that uses them.".into(),
        severity: Severity::Warning,
        span: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_with_path(code: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts_with_path(code, &Check, path)
    }

    #[test]
    fn flags_src_types() {
        assert_eq!(
            run_with_path("export type User = {}", "src/types.ts").len(),
            1
        );
    }

    #[test]
    fn flags_types_index() {
        assert_eq!(
            run_with_path("export type User = {}", "src/types/index.ts").len(),
            1
        );
    }

    #[test]
    fn flags_root_types() {
        assert_eq!(run_with_path("export type User = {}", "types.ts").len(), 1);
    }

    #[test]
    fn allows_domain_types() {
        assert!(run_with_path("export type User = {}", "src/users/types.ts").is_empty());
    }

    #[test]
    fn allows_colocated_types() {
        assert!(
            run_with_path("export type Props = {}", "src/components/Button.types.ts").is_empty()
        );
    }
}
