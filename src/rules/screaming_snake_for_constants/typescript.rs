#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use crate::rules::file_ctx::{FileCtx, PathSegments};
    use crate::rules::screaming_snake_for_constants::oxc_typescript::Check;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    fn run_in_storybook(source: &str) -> Vec<Diagnostic> {
        let file = FileCtx {
            path_segments: PathSegments { in_storybook: true, ..Default::default() },
            ..Default::default()
        };
        crate::rules::test_helpers::run_rule_with_ctx(
            &Check,
            source,
            "Button.stories.tsx",
            crate::project::default_static_project_ctx(),
            &file,
        )
    }

    #[test]
    fn flags_camel_case_numeric() {
        let diags = run("const maxRetries = 3;");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("maxRetries"));
    }

    #[test]
    fn allows_string_constant() {
        assert!(run("const apiUrl = \"https://example.com\";").is_empty());
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
    fn allows_call_expression() {
        assert!(run("const errorReporter = createReporter();").is_empty());
    }

    #[test]
    fn allows_object_literal() {
        assert!(run("const config = { a: 1 };").is_empty());
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

    #[test]
    fn flags_negative_number() {
        let diags = run("const minValue = -1;");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_new_expression() {
        assert!(run("const instance = new Map();").is_empty());
    }

    #[test]
    fn allows_exported_array_of_strings_config() {
        assert!(
            run("export const optimizeViteDeps = ['preact/compat/jsx-runtime', '@storybook/react-dom-shim'];")
                .is_empty()
        );
    }

    #[test]
    fn allows_array_of_strings_config() {
        assert!(run("const allowedOrigins = ['https://a.com', 'https://b.com'];").is_empty());
    }

    #[test]
    fn flags_numeric_array() {
        let diags = run("const retryDelays = [100, 200, 400];");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_empty_array_collection() {
        assert!(run("const listeners: Array<(state: State) => void> = [];").is_empty());
    }

    // Angular Router mandates `export const routes: Routes` in `app.routes.ts`
    // (issue #1718). The empty form and the populated form (an array of route
    // object literals) are both configuration collections, not scalar magic
    // constants, so neither is required to be SCREAMING_SNAKE_CASE.
    #[test]
    fn allows_angular_routes_empty() {
        assert!(run("export const routes: Routes = [];").is_empty());
    }

    #[test]
    fn allows_angular_routes_object_literals() {
        let src =
            "export const routes: Routes = [{ path: '', component: AppComponent }, { path: 'x', component: X }];";
        assert!(run(src).is_empty());
    }

    // Issue #1668: top-level constants in Storybook story files are story-argument
    // fixtures and framework-magic names, not application-wide invariants.
    #[test]
    fn allows_story_constants_in_storybook_file() {
        let src = "const arrayOptions = ['Foo', 'Bar', 'Baz'];\n\
                   export const __namedExportsOrder = ['Story1', 'Story2'];\n\
                   const maxRetries = 3;";
        assert!(run_in_storybook(src).is_empty());
    }

    #[test]
    fn flags_numeric_constant_in_non_story_file() {
        let diags = run("const maxRetries = 3;");
        assert_eq!(diags.len(), 1);
    }
}
