#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use crate::rules::screaming_snake_for_constants::oxc_typescript::Check;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_camel_case_top_level() {
        let diags = run("const maxRetries = 3;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("maxRetries"));
    }

    #[test]
    fn allows_screaming_snake() {
        assert!(run("const MAX_RETRIES = 3;").is_empty());
    }

    #[test]
    fn allows_function_assignment() {
        assert!(run("const handleClick = () => {};").is_empty());
    }

    #[test]
    fn allows_local_const() {
        assert!(run("function f() { const localVar = 1; }").is_empty());
    }

    #[test]
    fn flags_exported_camel_case() {
        let diags = run("export const maxRetries = 3;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_exported_screaming_snake() {
        assert!(run("export const MAX_RETRIES = 3;").is_empty());
    }
}
