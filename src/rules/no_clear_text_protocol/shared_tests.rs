//! Shared scenarios for no-clear-text-protocol.

use crate::diagnostic::Diagnostic;

struct Scenario {
    name: &'static str,
    expected_flagged: bool,
    rust: &'static str,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "real http URL — flagged",
        expected_flagged: true,
        rust: r#"fn f() { let u = "http://api.acme.io"; }"#,
    },
    Scenario {
        name: "real ftp URL — flagged",
        expected_flagged: true,
        rust: r#"fn f() { let u = "ftp://files.acme.io"; }"#,
    },
    Scenario {
        name: "https URL — not flagged",
        expected_flagged: false,
        rust: r#"fn f() { let u = "https://example.com"; }"#,
    },
    Scenario {
        name: "localhost dev URL — not flagged",
        expected_flagged: false,
        rust: r#"fn f() { let u = "http://localhost:3000"; }"#,
    },
    Scenario {
        name: "bare http:// prefix in detection logic — user FP",
        expected_flagged: false,
        rust: r#"fn check(text: &str) -> bool { text.contains("http://") || text.contains("https://") }"#,
    },
    Scenario {
        name: "bare http:// constant — not flagged",
        expected_flagged: false,
        rust: r#"const HTTP_PREFIX: &str = "http://";"#,
    },
    Scenario {
        name: "url in comment — not flagged",
        expected_flagged: false,
        rust: "// see http://example.com\nfn f() {}",
    },
    Scenario {
        name: "SVG xmlns namespace URI — not flagged",
        expected_flagged: false,
        rust: r#"fn f() { let ns = "http://www.w3.org/2000/svg"; }"#,
    },
    Scenario {
        name: ".test TLD (RFC 2606 reserved, Vitest setup env) — not flagged",
        expected_flagged: false,
        rust: r#"fn f() { let u = "http://example.test:3000"; }"#,
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
