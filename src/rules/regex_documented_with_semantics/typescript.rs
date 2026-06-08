#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use crate::rules::regex_documented_with_semantics::oxc_typescript::Check;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_undocumented_complex_regex_literal() {
        let diags = run(r#"const re = /^P(?:\d+Y)?(?:\d+M)?(?:\d+D)?$/;"#);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_documented_regex_literal() {
        let src = "// ISO 8601 duration regex\nconst re = /^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$/;";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_short_regex() {
        assert!(run(r#"const re = /^\d+$/;"#).is_empty());
    }
}
