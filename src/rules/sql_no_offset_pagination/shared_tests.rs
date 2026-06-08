//! Shared scenarios for sql-no-offset-pagination.

use crate::diagnostic::Diagnostic;

struct Scenario {
    name: &'static str,
    expected_flagged: bool,
    ts: &'static str,
    rust: &'static str,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "limit + offset in SQL string — flagged",
        expected_flagged: true,
        ts: r#"const q = "SELECT * FROM users LIMIT 10 OFFSET 100";"#,
        rust: r#"fn f() { let q = "SELECT * FROM users LIMIT 10 OFFSET 100"; }"#,
    },
    Scenario {
        name: "user FP — string array of identifiers",
        expected_flagged: false,
        ts: r#"const bases = ["delay", "offset", "width", "limit", "rate"];"#,
        rust: r#"fn f() { let bases = &["delay", "offset", "width", "limit", "rate"]; }"#,
    },
    Scenario {
        name: "comment with the pattern",
        expected_flagged: false,
        ts: "// SELECT ... LIMIT 10 OFFSET 100\nconst x = 1;",
        rust: "// SELECT ... LIMIT 10 OFFSET 100\nfn f() {}",
    },
    Scenario {
        name: "SQL with limit only — not flagged",
        expected_flagged: false,
        ts: r#"const q = "SELECT * FROM users LIMIT 10";"#,
        rust: r#"fn f() { let q = "SELECT * FROM users LIMIT 10"; }"#,
    },
    Scenario {
        name: "non-SQL prose with the words",
        expected_flagged: false,
        ts: r#"const x = "the limit is the offset of the field";"#,
        rust: r#"fn f() { let x = "the limit is the offset of the field"; }"#,
    },
];

fn run_ts(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_oxc_ts(src, &super::oxc_typescript::Check)
}

fn run_rust(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rust(src, &super::rust::Check)
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
