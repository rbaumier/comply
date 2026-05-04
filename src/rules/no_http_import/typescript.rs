#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use crate::rules::no_http_import::oxc_typescript::Check;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_http_import() {
        let diags = run("import { something } from 'http://cdn.example.com/lib.js';");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("http://"));
    }

    #[test]
    fn allows_https_import() {
        assert!(run("import { something } from 'https://cdn.example.com/lib.js';").is_empty());
    }

    #[test]
    fn allows_local_import() {
        assert!(run("import { foo } from './foo';").is_empty());
    }

    #[test]
    fn allows_npm_import() {
        assert!(run("import express from 'express';").is_empty());
    }

    #[test]
    fn flags_http_side_effect_import() {
        let diags = run("import 'http://cdn.example.com/polyfill.js';");
        assert_eq!(diags.len(), 1);
    }
}
