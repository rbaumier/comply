//! Shared scenarios for db-no-string-concat-sql.
//!
//! Rust uses `format!` macro invocations to build SQL; the scenarios
//! exercise the SQL-injection-risk verdict (yes / no) on those shapes.

use crate::diagnostic::Diagnostic;

struct Scenario {
    name: &'static str,
    expected_flagged: bool,
    rust: &'static str,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "SELECT with interpolation — flagged",
        expected_flagged: true,
        rust: r#"fn f(id: i32) { let q = format!("SELECT * FROM users WHERE id = {}", id); }"#,
    },
    Scenario {
        name: "non-SQL string concat",
        expected_flagged: false,
        rust: r#"fn f(name: &str) { let m = format!("hello {}", name); }"#,
    },
    Scenario {
        name: "format!/concat with from_utf8_lossy arg — user FP",
        expected_flagged: false,
        rust: r#"fn f(stderr: &[u8]) -> String { format!("failed to parse oxlint output: {}", String::from_utf8_lossy(stderr)) }"#,
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
