//! Shared NCLOC scenarios, run against BOTH the TS and Rust backends.
//! Each scenario pairs a semantic case with the exact NCLOC count the
//! scanner must produce on equivalent code in each language. A drift
//! between the two backends surfaces as a failing test on one side.

struct Scenario {
    name: &'static str,
    expected_ncloc: usize,
    rust: &'static str,
    ts: &'static str,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "one-line body",
        expected_ncloc: 1,
        rust: "fn f() -> i32 { 42 }",
        ts: "function f() { return 42; }",
    },
    Scenario {
        name: "three statements, no decoration",
        expected_ncloc: 5,
        rust: "fn f() {\n    let a = 1;\n    let b = 2;\n    let c = a + b;\n}",
        ts: "function f() {\n    let a = 1;\n    let b = 2;\n    let c = a + b;\n}",
    },
    Scenario {
        name: "blank lines ignored",
        expected_ncloc: 5,
        rust: "fn f() {\n    let a = 1;\n\n    let b = 2;\n\n    let c = a + b;\n}",
        ts: "function f() {\n    let a = 1;\n\n    let b = 2;\n\n    let c = a + b;\n}",
    },
    Scenario {
        name: "line comments ignored",
        expected_ncloc: 5,
        rust: "fn f() {\n    // head\n    let a = 1;\n    // mid\n    let b = 2;\n    let c = a + b;\n}",
        ts: "function f() {\n    // head\n    let a = 1;\n    // mid\n    let b = 2;\n    let c = a + b;\n}",
    },
    Scenario {
        name: "block comment spanning lines ignored",
        expected_ncloc: 3,
        rust: "fn f() {\n    /*\n     * multi-line\n     * block\n     */\n    let a = 1;\n}",
        ts: "function f() {\n    /*\n     * multi-line\n     * block\n     */\n    let a = 1;\n}",
    },
    Scenario {
        // 4 physical lines, all code — the trailing `// note` does NOT
        // drop its line from NCLOC, and the closing brace counts.
        name: "trailing comment does not drop the line",
        expected_ncloc: 4,
        rust: "fn f() {\n    let a = 1; // note\n    let b = 2;\n}",
        ts: "function f() {\n    let a = 1; // note\n    let b = 2;\n}",
    },
];

#[test]
fn rust_backend_matches_ts_backend_on_scenarios() {
    for s in SCENARIOS {
        let rust_result = super::rust::compute_source(s.rust);
        let ts_result = super::typescript::compute_source(s.ts);
        assert_eq!(
            rust_result.len(),
            1,
            "scenario `{}`: rust backend found {} functions, expected 1",
            s.name,
            rust_result.len()
        );
        assert_eq!(
            ts_result.len(),
            1,
            "scenario `{}`: ts backend found {} functions, expected 1",
            s.name,
            ts_result.len()
        );
        let (_, rust_ncloc) = &rust_result[0];
        let (_, ts_ncloc) = &ts_result[0];
        assert_eq!(
            *rust_ncloc, s.expected_ncloc,
            "scenario `{}`: rust expected {}, got {}",
            s.name, s.expected_ncloc, rust_ncloc
        );
        assert_eq!(
            *ts_ncloc, s.expected_ncloc,
            "scenario `{}`: ts expected {}, got {}",
            s.name, s.expected_ncloc, ts_ncloc
        );
    }
}
