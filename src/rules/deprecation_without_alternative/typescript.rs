#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use crate::rules::deprecation_without_alternative::oxc_typescript::Check;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_bare_deprecated_jsdoc() {
        let diags = run("/** @deprecated */\nfunction old() {}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_deprecated_on_own_line() {
        let diags = run("/**\n * @deprecated\n */\nfunction old() {}");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_deprecated_with_message() {
        assert!(run("/** @deprecated Use newFn instead */\nfunction old() {}").is_empty());
    }

    #[test]
    fn allows_deprecated_with_message_multiline() {
        assert!(run("/**\n * @deprecated Use newFn instead.\n */\nfunction old() {}").is_empty());
    }

    #[test]
    fn ignores_non_jsdoc() {
        assert!(run("const x = 1;").is_empty());
    }

    #[test]
    fn ignores_line_comment() {
        assert!(run("// @deprecated\nfunction old() {}").is_empty());
    }

    #[test]
    fn allows_bare_deprecated_in_test_file() {
        assert!(
            crate::rules::test_helpers::run_oxc_ts_with_path(
                "/** @deprecated */\nfunction old() {}",
                &Check,
                "src/user.test.ts",
            )
            .is_empty()
        );
    }
}
