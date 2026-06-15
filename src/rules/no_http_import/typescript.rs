#[cfg(test)]
mod tests {
    use crate::diagnostic::Diagnostic;
    use crate::rules::no_http_import::oxc_typescript::Check;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
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

    #[test]
    fn allows_http_localhost_with_port() {
        assert!(
            run("import x from 'http://localhost:4545/subdir/a.ts';").is_empty()
        );
    }

    #[test]
    fn allows_http_loopback_ip_with_port() {
        assert!(run("import x from 'http://127.0.0.1:8000/b.ts';").is_empty());
    }

    #[test]
    fn allows_http_localhost_without_port() {
        assert!(run("import 'http://localhost/c.ts';").is_empty());
    }

    #[test]
    fn allows_http_localhost_type_only_import() {
        assert!(
            run("import type { } from 'http://localhost:4545/subdir/type_error.ts';").is_empty()
        );
    }

    #[test]
    fn flags_remote_http_import() {
        let diags = run("import x from 'http://example.com/a.ts';");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn flags_remote_http_with_localhost_in_path() {
        let diags = run("import x from 'http://evil.com/localhost/a.ts';");
        assert_eq!(diags.len(), 1);
    }
}
