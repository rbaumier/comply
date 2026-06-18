//! Shared scenarios for prefer-immediate-return.

use crate::diagnostic::Diagnostic;

struct Scenario {
    name: &'static str,
    expected_flagged: bool,
    rust: &'static str,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "assign then return — flagged",
        expected_flagged: true,
        rust: "fn f() -> i32 { let result = compute(); return result; }",
    },
    Scenario {
        name: "assign then tail expression (Rust only)",
        expected_flagged: true,
        rust: "fn f() -> i32 { let result = compute(); result }",
    },
    Scenario {
        name: "assign, used in method chain, return — user FP",
        expected_flagged: false,
        rust: r#"
            fn run() -> Parser {
                let mut parser = Parser::new();
                parser.set_language(&Lang).unwrap();
                parser
            }
        "#,
    },
    Scenario {
        name: "different variable returned",
        expected_flagged: false,
        rust: "fn f() -> i32 { let result = compute(); return other; }",
    },
    Scenario {
        name: "destructuring pattern not flagged",
        expected_flagged: false,
        rust: "fn f() -> i32 { let (a, b) = pair(); return a; }",
    },
];

fn run_rust(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rule(&super::rust::Check, src, "t.rs")
}

#[test]
fn rust_backend_matches_spec() {
    for s in SCENARIOS {
        let flagged = !run_rust(s.rust).is_empty();
        assert_eq!(
            flagged, s.expected_flagged,
            "rust scenario `{}`: expected flagged={}, got flagged={} (source: {:?})",
            s.name, s.expected_flagged, flagged, s.rust
        );
    }
}
