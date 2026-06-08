//! Shared scenarios for no-duplicate-string.

use crate::diagnostic::Diagnostic;

struct Scenario {
    name: &'static str,
    expected_flagged: bool,
    ts: &'static str,
    rust: &'static str,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "string appearing 3 times — flagged",
        expected_flagged: true,
        ts: "const a = \"hello world\"; const b = \"hello world\"; const c = \"hello world\";",
        rust: "fn f() { let a = \"hello world\"; let b = \"hello world\"; let c = \"hello world\"; }",
    },
    Scenario {
        name: "string appearing 2 times — not flagged",
        expected_flagged: false,
        ts: "const a = \"long enough string\"; const b = \"long enough string\";",
        rust: "fn f() { let a = \"long enough string\"; let b = \"long enough string\"; }",
    },
    Scenario {
        name: "short string appearing 3 times — not flagged",
        expected_flagged: false,
        ts: "const a = \"short\"; const b = \"short\"; const c = \"short\";",
        rust: "fn f() { let a = \"short\"; let b = \"short\"; let c = \"short\"; }",
    },
    Scenario {
        name: "string only appearing in comments — not flagged",
        expected_flagged: false,
        ts: "// \"structured_output\" is the field\n// fallback \"structured_output\"\n// finally \"structured_output\"\nconst field = \"structured_output\";",
        rust: "// \"structured_output\" is the field\n// fallback \"structured_output\"\n// finally \"structured_output\"\nfn f() { let field = \"structured_output\"; }",
    },
];

fn run_ts(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rule(&super::oxc_typescript::Check, src, "t.ts")
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
