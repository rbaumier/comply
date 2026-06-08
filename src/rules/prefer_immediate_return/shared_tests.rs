//! Shared scenarios for prefer-immediate-return.

use crate::diagnostic::Diagnostic;

struct Scenario {
    name: &'static str,
    expected_flagged: bool,
    ts: Option<&'static str>,
    rust: Option<&'static str>,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "assign then return — flagged",
        expected_flagged: true,
        ts: Some("function f() { const result = computeValue(); return result; }"),
        rust: Some("fn f() -> i32 { let result = compute(); return result; }"),
    },
    Scenario {
        name: "assign then tail expression (Rust only)",
        expected_flagged: true,
        ts: None,
        rust: Some("fn f() -> i32 { let result = compute(); result }"),
    },
    Scenario {
        name: "assign, used in method chain, return — user FP",
        expected_flagged: false,
        ts: Some(
            r#"
            function run() {
                const parser = new Parser();
                parser.setLanguage(Lang.TypeScript);
                return parser;
            }
        "#,
        ),
        rust: Some(
            r#"
            fn run() -> Parser {
                let mut parser = Parser::new();
                parser.set_language(&Lang).unwrap();
                parser
            }
        "#,
        ),
    },
    Scenario {
        name: "different variable returned",
        expected_flagged: false,
        ts: Some("function f() { const result = compute(); return other; }"),
        rust: Some("fn f() -> i32 { let result = compute(); return other; }"),
    },
    Scenario {
        name: "destructuring pattern not flagged",
        expected_flagged: false,
        ts: Some("function f() { const { a, b } = getValues(); return a; }"),
        rust: Some("fn f() -> i32 { let (a, b) = pair(); return a; }"),
    },
];

fn run_ts(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rule(&super::typescript::Check, src, "t.ts")
}

fn run_rust(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rule(&super::rust::Check, src, "t.rs")
}

#[test]
fn typescript_backend_matches_spec() {
    for s in SCENARIOS {
        let Some(src) = s.ts else { continue };
        let flagged = !run_ts(src).is_empty();
        assert_eq!(
            flagged, s.expected_flagged,
            "ts scenario `{}`: expected flagged={}, got flagged={} (source: {:?})",
            s.name, s.expected_flagged, flagged, src
        );
    }
}

#[test]
fn rust_backend_matches_spec() {
    for s in SCENARIOS {
        let Some(src) = s.rust else { continue };
        let flagged = !run_rust(src).is_empty();
        assert_eq!(
            flagged, s.expected_flagged,
            "rust scenario `{}`: expected flagged={}, got flagged={} (source: {:?})",
            s.name, s.expected_flagged, flagged, src
        );
    }
}
