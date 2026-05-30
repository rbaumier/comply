//! Shared scenarios for no-clear-text-protocol.

use crate::diagnostic::Diagnostic;

struct Scenario {
    name: &'static str,
    expected_flagged: bool,
    ts: &'static str,
    rust: &'static str,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "real http URL — flagged",
        expected_flagged: true,
        ts: r#"const u = "http://api.acme.io";"#,
        rust: r#"fn f() { let u = "http://api.acme.io"; }"#,
    },
    Scenario {
        name: "real ftp URL — flagged",
        expected_flagged: true,
        ts: r#"const u = "ftp://files.acme.io";"#,
        rust: r#"fn f() { let u = "ftp://files.acme.io"; }"#,
    },
    Scenario {
        name: "https URL — not flagged",
        expected_flagged: false,
        ts: r#"const u = "https://example.com";"#,
        rust: r#"fn f() { let u = "https://example.com"; }"#,
    },
    Scenario {
        name: "localhost dev URL — not flagged",
        expected_flagged: false,
        ts: r#"const u = "http://localhost:3000";"#,
        rust: r#"fn f() { let u = "http://localhost:3000"; }"#,
    },
    Scenario {
        name: "bare http:// prefix in detection logic — user FP",
        expected_flagged: false,
        ts: r#"if (text.includes("http://") || text.includes("https://")) {}"#,
        rust: r#"fn check(text: &str) -> bool { text.contains("http://") || text.contains("https://") }"#,
    },
    Scenario {
        name: "bare http:// constant — not flagged",
        expected_flagged: false,
        ts: r#"const HTTP_PREFIX = "http://";"#,
        rust: r#"const HTTP_PREFIX: &str = "http://";"#,
    },
    Scenario {
        name: "url in comment — not flagged",
        expected_flagged: false,
        ts: "// see http://example.com\nconst x = 1;",
        rust: "// see http://example.com\nfn f() {}",
    },
    Scenario {
        name: "SVG xmlns namespace URI — not flagged",
        expected_flagged: false,
        ts: r#"const ns = "http://www.w3.org/2000/svg";"#,
        rust: r#"fn f() { let ns = "http://www.w3.org/2000/svg"; }"#,
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
