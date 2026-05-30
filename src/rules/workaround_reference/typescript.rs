#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use crate::rules::workaround_reference::oxc_typescript::Check;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_workaround_without_ref() {
        let diags = run("// Workaround for fish\nconst x = 1;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_workaround_with_issue_ref() {
        assert!(run("// Workaround for a fish bug (see #739, #279)\nconst x = 1;").is_empty());
    }

    #[test]
    fn allows_workaround_with_url() {
        assert!(
            run("// Workaround for https://github.com/org/repo/issues/1\nconst x = 1;")
                .is_empty()
        );
    }

    #[test]
    fn flags_hack_without_ref() {
        let diags = run("// hack to fix rendering\nconst x = 1;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_jira_ref() {
        assert!(run("// Workaround for PROJ-123\nconst x = 1;").is_empty());
    }

    #[test]
    fn no_fp_on_compatible_type_description() {
        // "structurally compatible with RelationalWhere<T>" — pure type-system term, not a workaround
        let src = r#"
/**
 * The returned shape is structurally compatible with `RelationalWhere<TTable>`
 * for every table that declares a `deactivatedAt` column.
 */
const x = 1;
"#;
        assert!(run(src).is_empty());
    }

    #[test]
    fn no_fp_on_incompatible() {
        assert!(run("// These APIs are incompatible with each other\nconst x = 1;").is_empty());
    }

    #[test]
    fn flags_compat_layer() {
        let diags = run("// compat layer for old browsers\nconst x = 1;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_compat_fix() {
        let diags = run("// compat fix for Safari\nconst x = 1;");
        assert_eq!(diags.len(), 1);
    }
}
