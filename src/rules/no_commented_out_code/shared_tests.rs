//! Shared scenarios for no-commented-out-code.
//!
//! Each scenario pairs a semantic case with the expected verdict
//! (flagged or not). Cross-checks that the TS and Rust backends agree
//! on the same category of comment. When a cross-lang equivalence
//! does not exist (e.g. a Rust macro has no TS counterpart), the
//! corresponding side is set to `None` and only the other backend
//! runs on the scenario.

use crate::diagnostic::Diagnostic;

struct Scenario {
    name: &'static str,
    expected_flagged: bool,
    ts: Option<&'static str>,
    rust: Option<&'static str>,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "single declaration",
        expected_flagged: true,
        ts: Some("// const x = 5;"),
        rust: Some("// let x = 5;"),
    },
    Scenario {
        name: "single function call",
        expected_flagged: true,
        ts: Some("// foo(bar);"),
        rust: Some("// foo(bar);"),
    },
    Scenario {
        name: "adjacent declarations",
        expected_flagged: true,
        ts: Some("// const x = 5;\n// const y = 10;"),
        rust: Some("// let x = 5;\n// let y = 10;"),
    },
    Scenario {
        name: "prose comment",
        expected_flagged: false,
        ts: Some("// This function computes the total cost."),
        rust: Some("// This function computes the total cost."),
    },
    Scenario {
        name: "user false positive — pattern list with trailing =",
        expected_flagged: false,
        ts: Some("// const foo =, let foo =, var foo ="),
        rust: Some("// let foo =, const foo =, static foo ="),
    },
    Scenario {
        name: "short label comment",
        expected_flagged: false,
        ts: Some("// setup"),
        rust: Some("// setup"),
    },
    Scenario {
        name: "doc comment with example code",
        expected_flagged: false,
        ts: Some("/// const x = 5;"),
        rust: Some("/// let x = 5;"),
    },
    Scenario {
        name: "block comment with real code",
        expected_flagged: true,
        ts: Some("/* const x = 5; foo(x); */"),
        rust: Some("/* let x = 5; foo(x); */"),
    },
    Scenario {
        name: "jsdoc / rustdoc block",
        expected_flagged: false,
        ts: Some("/** @returns cost */"),
        rust: Some("/** doc comment */"),
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
            "ts scenario `{}`: expected flagged={}, got flagged={} (source: {src:?})",
            s.name, s.expected_flagged, flagged
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
            "rust scenario `{}`: expected flagged={}, got flagged={} (source: {src:?})",
            s.name, s.expected_flagged, flagged
        );
    }
}
