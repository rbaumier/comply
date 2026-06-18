//! Shared scenarios for no-commented-out-code.
//!
//! Each scenario pairs a semantic case with the expected verdict
//! (flagged or not) for the Rust backend.

use crate::diagnostic::Diagnostic;

struct Scenario {
    name: &'static str,
    expected_flagged: bool,
    rust: &'static str,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "single declaration",
        expected_flagged: true,
        rust: "// let x = 5;",
    },
    Scenario {
        name: "single function call",
        expected_flagged: true,
        rust: "// foo(bar);",
    },
    Scenario {
        name: "adjacent declarations",
        expected_flagged: true,
        rust: "// let x = 5;\n// let y = 10;",
    },
    Scenario {
        name: "prose comment",
        expected_flagged: false,
        rust: "// This function computes the total cost.",
    },
    Scenario {
        name: "user false positive — pattern list with trailing =",
        expected_flagged: false,
        rust: "// let foo =, const foo =, static foo =",
    },
    Scenario {
        name: "short label comment",
        expected_flagged: false,
        rust: "// setup",
    },
    Scenario {
        name: "doc comment with example code",
        expected_flagged: false,
        rust: "/// let x = 5;",
    },
    Scenario {
        name: "block comment with real code",
        expected_flagged: true,
        rust: "/* let x = 5; foo(x); */",
    },
    Scenario {
        name: "jsdoc / rustdoc block",
        expected_flagged: false,
        rust: "/** doc comment */",
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
