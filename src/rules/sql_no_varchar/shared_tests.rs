//! Shared scenarios for sql-no-varchar.

use crate::diagnostic::Diagnostic;

struct Scenario {
    name: &'static str,
    expected_flagged: bool,
    rust: &'static str,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "varchar in CREATE TABLE — flagged",
        expected_flagged: true,
        rust: r#"fn f() { let m = "CREATE TABLE users (name VARCHAR(255))"; }"#,
    },
    Scenario {
        name: "char in ALTER TABLE — flagged",
        expected_flagged: true,
        rust: r#"fn f() { let m = "ALTER TABLE users ADD COLUMN code CHAR(3)"; }"#,
    },
    Scenario {
        name: "test fn name with _char( — user FP",
        expected_flagged: false,
        rust: "fn flags_negative_lookahead_same_char() { let x = 1; }",
    },
    Scenario {
        name: "TEXT column instead — not flagged",
        expected_flagged: false,
        rust: r#"fn f() { let m = "CREATE TABLE users (name TEXT)"; }"#,
    },
    Scenario {
        name: "comment with the pattern",
        expected_flagged: false,
        rust: "// CREATE TABLE users (name VARCHAR(255))\nfn f() {}",
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
