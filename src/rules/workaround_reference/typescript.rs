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
}
