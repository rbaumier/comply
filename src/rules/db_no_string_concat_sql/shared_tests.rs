//! Shared scenarios for db-no-string-concat-sql.
//!
//! TS uses `binary_expression` (`+` concat) while Rust uses
//! `format!` macro invocations. The scenarios are not always 1:1
//! because the syntactic shapes differ — only the SEMANTICS map
//! (SQL injection risk yes / no).

use crate::diagnostic::Diagnostic;

struct Scenario {
    name: &'static str,
    expected_flagged: bool,
    ts: Option<&'static str>,
    rust: Option<&'static str>,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "SELECT with interpolation — flagged",
        expected_flagged: true,
        ts: Some(r#"const q = "SELECT * FROM users WHERE id = " + userId;"#),
        rust: Some(
            r#"fn f(id: i32) { let q = format!("SELECT * FROM users WHERE id = {}", id); }"#,
        ),
    },
    Scenario {
        name: "non-SQL string concat",
        expected_flagged: false,
        ts: Some(r#"const m = "hello " + name;"#),
        rust: Some(r#"fn f(name: &str) { let m = format!("hello {}", name); }"#),
    },
    Scenario {
        name: "format!/concat with from_utf8_lossy arg — user FP",
        expected_flagged: false,
        ts: None, // TS-side equivalent doesn't exist
        rust: Some(
            r#"fn f(stderr: &[u8]) -> String { format!("failed to parse oxlint output: {}", String::from_utf8_lossy(stderr)) }"#,
        ),
    },
    Scenario {
        name: "string concat with userFromDb identifier — TS FP family",
        expected_flagged: false,
        ts: Some(r#"const m = "the result was " + userFromDb;"#),
        rust: None,
    },
    Scenario {
        name: "parameterised SELECT — not flagged",
        expected_flagged: false,
        ts: Some(r#"const q = "SELECT * FROM users WHERE id = $1";"#),
        rust: None,
    },
];

fn run_ts(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_ts(src, &super::typescript::Check)
}

fn run_rust(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rust(src, &super::rust::Check)
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
