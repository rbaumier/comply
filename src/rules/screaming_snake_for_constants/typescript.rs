#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use crate::rules::file_ctx::{FileCtx, PathSegments};
    use crate::rules::screaming_snake_for_constants::oxc_typescript::Check;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    fn run_at(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    fn run_gated(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule_gated(&Check, source, path)
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

    // Issue #1586: SvelteKit page options are `const` exports whose lowercase
    // names are mandated by the framework; SvelteKit reads them by exact name,
    // so they cannot be SCREAMING_SNAKE_CASE.
    #[test]
    fn allows_sveltekit_page_options_in_route_file() {
        let src = "export const prerender = true;\n\
                   export const ssr = false;\n\
                   export const csr = false;";
        assert!(run_at(src, "src/routes/spa-shell/+page.ts").is_empty());
        assert!(run_at(src, "src/routes/prerendered/+page.server.ts").is_empty());
    }

    // Negative space 1: a genuine lowercase magic constant in a SvelteKit route
    // file is NOT a reserved page option, so it must still fire.
    #[test]
    fn flags_non_page_option_const_in_route_file() {
        let diags = run_at("export const maxRetries = 5;", "src/routes/x/+page.ts");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("maxRetries"));
    }

    // Negative space 2: the same reserved name outside a SvelteKit route file is
    // an ordinary constant and must still fire.
    #[test]
    fn flags_page_option_name_outside_route_file() {
        let diags = run_at("export const prerender = true;", "src/lib/options.ts");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("prerender"));
    }

    // Issue #1591: boolean mock config flags in a mock/fixture file mirror the
    // camelCase property names of the runtime config object they simulate, so
    // they are exempt from SCREAMING_SNAKE_CASE.
    #[test]
    fn allows_mock_config_flags_in_mock_file() {
        let src = "export const asyncCallHook = true;\n\
                   export const clientNodePlaceholder = false;\n\
                   export const hasPluginDependencies = true;\n\
                   export const componentIslands = true;";
        assert!(run_at(src, "test/mocks/nuxt-config.ts").is_empty());
        assert!(run_at(src, "src/__fixtures__/config.ts").is_empty());
    }

    // Negative space: a genuine top-level primitive constant in production source
    // is unaffected by the mock/fixture exemption and must still fire.
    #[test]
    fn flags_numeric_constant_in_production_source() {
        let diags = run_at("export const maxRetries = 5;", "src/lib/retry.ts");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("maxRetries"));
    }

    // Issue #1221: top-level constants in tsd `test-d/` type-test files are
    // type-check fixtures with intentional camelCase names, not runtime
    // constants. The central test-dir gate (`skip_in_test_dir`) suppresses the
    // rule there, so these produce no diagnostics.
    #[test]
    fn allows_type_test_fixtures_in_test_d_dir() {
        let src = "const objectExample = {a: 1};\n\
                   const arrayEntryString = [0, 'a'];";
        assert!(run_gated(src, "test-d/entries.ts").is_empty());
    }

    // The gate covers the broader test-dir set, not just `test-d/`.
    #[test]
    fn allows_fixture_const_in_dot_test_file() {
        assert!(run_gated("const maxRetries = 3;", "foo.test.ts").is_empty());
    }

    // Negative space: the test-dir gate only silences the rule inside test
    // directories. A genuine non-SCREAMING constant in production source still
    // fires through the gate.
    #[test]
    fn flags_production_const_through_gate() {
        let diags = run_gated("const apiTimeout = 5000;", "src/config.ts");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("apiTimeout"));
    }
}
