//! Shared no-type-encoded-names scenarios.
//!
//! Each scenario pairs a snake_case Rust source with the equivalent
//! camelCase TypeScript source and asserts both backends agree on
//! the verdict (flagged or not). The shared `TYPE_PREFIXES` list in
//! `type_prefix.rs` means a drift between the two backends is almost
//! always a bug in one of them, not a difference of intent.

use crate::diagnostic::Diagnostic;

struct Scenario {
    name: &'static str,
    expected_rust: bool,
    expected_ts: bool,
    rust: &'static str,
    ts: &'static str,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "str prefix — domain qualifier in Rust, Hungarian in TS",
        expected_rust: false,
        expected_ts: true,
        rust: "fn f() { let str_value = String::new(); }",
        ts: "const strValue = 'x';",
    },
    Scenario {
        name: "arr prefix — domain qualifier in Rust, Hungarian in TS",
        expected_rust: false,
        expected_ts: true,
        rust: "fn f() { let arr_items: Vec<i32> = vec![]; }",
        ts: "const arrItems = [];",
    },
    Scenario {
        name: "bool prefix — domain qualifier in Rust, Hungarian in TS",
        expected_rust: false,
        expected_ts: true,
        rust: "fn f() { let bool_flag = true; }",
        ts: "const boolFlag = true;",
    },
    Scenario {
        name: "obj prefix — domain qualifier in Rust, Hungarian in TS",
        expected_rust: false,
        expected_ts: true,
        rust: "fn f() { let obj_user = (); }",
        ts: "const objUser = {};",
    },
    Scenario {
        name: "dbl prefix — legacy Hungarian",
        expected_rust: true,
        expected_ts: true,
        rust: "fn f() { let dbl_value = 3.14; }",
        ts: "const dblValue = 3.14;",
    },
    Scenario {
        name: "fn — descriptive, NOT Hungarian",
        expected_rust: false,
        expected_ts: false,
        rust: "fn f() { let fn_name = String::new(); }",
        ts: "const fnCallback = () => {};",
    },
    Scenario {
        name: "num — descriptive, NOT Hungarian",
        expected_rust: false,
        expected_ts: false,
        rust: "fn f() { let num_items = 5; }",
        ts: "const numItems = 5;",
    },
    Scenario {
        name: "int — descriptive, NOT Hungarian",
        expected_rust: false,
        expected_ts: false,
        rust: "fn f() { let int_count = 0; }",
        ts: "const intCount = 0;",
    },
    Scenario {
        name: "descriptive name with no prefix",
        expected_rust: false,
        expected_ts: false,
        rust: "fn f() { let user_name = String::new(); }",
        ts: "const userName = 'x';",
    },
];

fn run_rust(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rust(src, &super::rust::Check)
}

fn run_ts(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_oxc_ts(src, &super::oxc_typescript::Check)
}

#[test]
fn rust_backend_matches_spec() {
    for s in SCENARIOS {
        let flagged = !run_rust(s.rust).is_empty();
        assert_eq!(
            flagged, s.expected_rust,
            "rust scenario `{}`: expected flagged={}, got flagged={} (source: {:?})",
            s.name, s.expected_rust, flagged, s.rust
        );
    }
}

#[test]
fn typescript_backend_matches_spec() {
    for s in SCENARIOS {
        let flagged = !run_ts(s.ts).is_empty();
        assert_eq!(
            flagged, s.expected_ts,
            "ts scenario `{}`: expected flagged={}, got flagged={} (source: {:?})",
            s.name, s.expected_ts, flagged, s.ts
        );
    }
}
