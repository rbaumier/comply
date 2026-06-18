//! Shared no-type-encoded-names scenarios.
//!
//! Each scenario pairs a snake_case Rust source with the expected
//! verdict (flagged or not). The `TYPE_PREFIXES` list in
//! `type_prefix.rs` drives the detection.

use crate::diagnostic::Diagnostic;

struct Scenario {
    name: &'static str,
    expected_rust: bool,
    rust: &'static str,
}

const SCENARIOS: &[Scenario] = &[
    Scenario {
        name: "str prefix — domain qualifier in Rust",
        expected_rust: false,
        rust: "fn f() { let str_value = String::new(); }",
    },
    Scenario {
        name: "arr prefix — domain qualifier in Rust",
        expected_rust: false,
        rust: "fn f() { let arr_items: Vec<i32> = vec![]; }",
    },
    Scenario {
        name: "bool prefix — domain qualifier in Rust",
        expected_rust: false,
        rust: "fn f() { let bool_flag = true; }",
    },
    Scenario {
        name: "obj prefix — domain qualifier in Rust",
        expected_rust: false,
        rust: "fn f() { let obj_user = (); }",
    },
    Scenario {
        name: "dbl prefix — legacy Hungarian",
        expected_rust: true,
        rust: "fn f() { let dbl_value = 3.14; }",
    },
    Scenario {
        name: "fn — descriptive, NOT Hungarian",
        expected_rust: false,
        rust: "fn f() { let fn_name = String::new(); }",
    },
    Scenario {
        name: "num — descriptive, NOT Hungarian",
        expected_rust: false,
        rust: "fn f() { let num_items = 5; }",
    },
    Scenario {
        name: "int — descriptive, NOT Hungarian",
        expected_rust: false,
        rust: "fn f() { let int_count = 0; }",
    },
    Scenario {
        name: "descriptive name with no prefix",
        expected_rust: false,
        rust: "fn f() { let user_name = String::new(); }",
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
            flagged, s.expected_rust,
            "rust scenario `{}`: expected flagged={}, got flagged={} (source: {:?})",
            s.name, s.expected_rust, flagged, s.rust
        );
    }
}
