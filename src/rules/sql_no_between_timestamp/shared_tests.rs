//! Shared scenarios for sql-no-between-timestamp.
//!
//! Each scenario pairs equivalent TypeScript and Rust code with the
//! expected verdict. Cross-checks that both backends agree, so a
//! drift in one (e.g. the helper `sql_uses_between_on_timestamp` not
//! matching a column name in one language but not the other) shows
//! up as a failing test.

use crate::diagnostic::Diagnostic;

struct Scenario {
    name: &'static str,
    expected_flagged: bool,
    ts: &'static str,
    rust: &'static str,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "between on created_at — flagged",
        expected_flagged: true,
        ts: r#"const q = "SELECT * FROM events WHERE created_at BETWEEN '2024-01-01' AND '2024-12-31'";"#,
        rust: r#"fn f() { let q = "SELECT * FROM events WHERE created_at BETWEEN '2024-01-01' AND '2024-12-31'"; }"#,
    },
    Scenario {
        name: "between on id — not flagged",
        expected_flagged: false,
        ts: r#"const q = "SELECT * FROM users WHERE id BETWEEN 1 AND 100";"#,
        rust: r#"fn f() { let q = "SELECT * FROM users WHERE id BETWEEN 1 AND 100"; }"#,
    },
    Scenario {
        name: "comment with the pattern — not flagged",
        expected_flagged: false,
        ts: "// WHERE created_at BETWEEN start AND end\nconst x = 1;",
        rust: "// WHERE created_at BETWEEN start AND end\nfn f() {}",
    },
    Scenario {
        name: "identifier with `between` — not flagged",
        expected_flagged: false,
        ts: "const between_timestamps = true;",
        rust: "fn f() { let between_timestamps = true; }",
    },
    Scenario {
        name: "non-SQL prose with the words — not flagged",
        expected_flagged: false,
        ts: r#"const x = "user selected items delivered from store between two timestamps";"#,
        rust: r#"fn f() { let x = "user selected items delivered from store between two timestamps"; }"#,
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
        let flagged = !run_ts(s.ts).is_empty();
        assert_eq!(
            flagged, s.expected_flagged,
            "ts scenario `{}`: expected flagged={}, got flagged={} (source: {:?})",
            s.name, s.expected_flagged, flagged, s.ts
        );
    }
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
