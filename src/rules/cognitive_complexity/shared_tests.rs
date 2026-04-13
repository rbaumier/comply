//! Shared cognitive-complexity scenarios, run against BOTH the Rust and
//! TypeScript backends. Each scenario pairs a semantic case with the exact
//! score it should produce per the SonarSource Cognitive Complexity white
//! paper (https://www.sonarsource.com/resources/cognitive-complexity/).
//!
//! The table lives here so any drift between the two backends surfaces as a
//! failing test on one language, and so a regression in the calculator is
//! immediately visible as "scenario X: expected N, got M".

struct Scenario {
    name: &'static str,
    expected: u32,
    rust: &'static str,
    ts: &'static str,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "empty function",
        expected: 0,
        rust: "fn f() {}",
        ts: "function f() {}",
    },
    Scenario {
        name: "single if",
        expected: 1,
        rust: "fn f(x: i32) { if x > 0 { foo(); } }",
        ts: "function f(x) { if (x > 0) { foo(); } }",
    },
    Scenario {
        name: "if/else (plain)",
        expected: 2,
        rust: "fn f(x: i32) { if x > 0 { a(); } else { b(); } }",
        ts: "function f(x) { if (x > 0) { a(); } else { b(); } }",
    },
    Scenario {
        name: "nested if (+1 outer, +2 inner)",
        expected: 3,
        rust: "fn f(x: i32, y: i32) { if x > 0 { if y > 0 { foo(); } } }",
        ts: "function f(x, y) { if (x > 0) { if (y > 0) { foo(); } } }",
    },
    Scenario {
        name: "single for loop",
        expected: 1,
        rust: "fn f() { for i in 0..10 { foo(i); } }",
        ts: "function f() { for (let i = 0; i < 10; i++) { foo(i); } }",
    },
    Scenario {
        name: "if inside for (+1 for, +2 if)",
        expected: 3,
        rust: "fn f() { for i in 0..10 { if i > 5 { foo(); } } }",
        ts: "function f() { for (let i = 0; i < 10; i++) { if (i > 5) { foo(); } } }",
    },
    Scenario {
        // The reported regression: a bare `match`/`switch` with three arms
        // must score exactly 1 — arms are continuations, not flow points.
        name: "match/switch with three arms scores 1",
        expected: 1,
        rust: "fn f(x: i32) -> i32 { match x { 0 => 1, 1 => 2, _ => 3 } }",
        ts: "function f(x) { switch (x) { case 0: return 1; case 1: return 2; default: return 3; } }",
    },
    Scenario {
        name: "match/switch with many arms still scores 1",
        expected: 1,
        rust: "fn f(x: i32) -> i32 { match x { 0 => 1, 1 => 2, 2 => 3, 3 => 4, 4 => 5, _ => 0 } }",
        ts: "function f(x) { switch (x) { case 0: return 1; case 1: return 2; case 2: return 3; case 3: return 4; case 4: return 5; default: return 0; } }",
    },
    Scenario {
        name: "if with && in condition (+1 if, +1 operator)",
        expected: 2,
        rust: "fn f(x: i32, y: i32) { if x > 0 && y > 0 { foo(); } }",
        ts: "function f(x, y) { if (x > 0 && y > 0) { foo(); } }",
    },
    Scenario {
        // Exact reproduction of src/main.rs:50 — the user's original bug
        // report. Must score 1: a single match with three arms, no nested
        // flow inside any arm.
        name: "main() match with Ok/Ok/Err arms",
        expected: 1,
        rust: r#"fn main() -> i32 {
    match run() {
        Ok(true) => 1,
        Ok(false) => 0,
        Err(e) => {
            eprintln!("err: {e}");
            2
        }
    }
}"#,
        ts: r#"function main() {
    switch (run()) {
        case "ok-true": return 1;
        case "ok-false": return 0;
        default:
            console.error("err");
            return 2;
    }
}"#,
    },
];

#[test]
fn rust_backend_matches_spec() {
    for s in SCENARIOS {
        let got = super::rust::compute_source(s.rust);
        assert_eq!(
            got, s.expected,
            "rust backend, scenario `{}`: expected {}, got {}",
            s.name, s.expected, got
        );
    }
}

#[test]
fn typescript_backend_matches_spec() {
    for s in SCENARIOS {
        let got = super::typescript::compute_source(s.ts);
        assert_eq!(
            got, s.expected,
            "typescript backend, scenario `{}`: expected {}, got {}",
            s.name, s.expected, got
        );
    }
}
