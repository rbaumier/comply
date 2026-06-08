//! Cross-backend scenarios for `nested-control-flow`.
//!
//! Each scenario is expressed both in Rust and in TS/JS and must produce
//! the same verdict (diagnostic count) on both backends.

#![cfg(test)]

use crate::diagnostic::Diagnostic;

fn run_rs(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_rust(src, &super::rust::Check)
}

fn run_ts(src: &str) -> Vec<Diagnostic> {
    crate::rules::test_helpers::run_oxc_ts(src, &super::oxc_typescript::Check)
}

#[test]
fn flat_else_if_cascade_is_one_level() {
    let rs = r#"
fn f(x: i32) -> i32 {
    if x == 0 { 0 }
    else if x == 1 { 1 }
    else if x == 2 { 2 }
    else if x == 3 { 3 }
    else if x == 4 { 4 }
    else { -1 }
}
"#;
    let ts = r#"
function f(x) {
    if (x === 0) return 0;
    else if (x === 1) return 1;
    else if (x === 2) return 2;
    else if (x === 3) return 3;
    else if (x === 4) return 4;
    else return -1;
}
"#;
    assert!(run_rs(rs).is_empty(), "Rust: else-if cascade flagged");
    assert!(run_ts(ts).is_empty(), "TS: else-if cascade flagged");
}

#[test]
fn triple_nested_ifs_stays_under_threshold() {
    let rs = r#"
fn f() {
    if a() {
        if b() {
            if c() {
                d();
            }
        }
    }
}
"#;
    let ts = r#"
function f() {
    if (a()) {
        if (b()) {
            if (c()) {
                d();
            }
        }
    }
}
"#;
    assert!(run_rs(rs).is_empty());
    assert!(run_ts(ts).is_empty());
}

#[test]
fn four_nested_ifs_flagged() {
    let rs = r#"
fn f() {
    if a() {
        if b() {
            if c() {
                if d() {
                    e();
                }
            }
        }
    }
}
"#;
    let ts = r#"
function f() {
    if (a()) {
        if (b()) {
            if (c()) {
                if (d()) {
                    e();
                }
            }
        }
    }
}
"#;
    assert_eq!(run_rs(rs).len(), 1, "Rust: expected 1 diag");
    assert_eq!(run_ts(ts).len(), 1, "TS: expected 1 diag");
}

#[test]
fn callable_boundary_resets_depth() {
    // A 3-deep nesting followed by a callable whose body is also 3-deep
    // must not flag either site.
    let rs = r#"
fn outer() {
    for _ in 0..10 {
        for _ in 0..10 {
            for _ in 0..10 {
                let cb = |x: u8| {
                    if x > 0 {
                        if x > 1 {
                            if x > 2 {
                                go();
                            }
                        }
                    }
                };
                cb(0);
            }
        }
    }
}
"#;
    let ts = r#"
function outer() {
    for (const _ of a) {
        for (const _ of b) {
            for (const _ of c) {
                const cb = (x) => {
                    if (x > 0) {
                        if (x > 1) {
                            if (x > 2) {
                                go();
                            }
                        }
                    }
                };
                cb(0);
            }
        }
    }
}
"#;
    assert!(
        run_rs(rs).is_empty(),
        "Rust: callable boundary not resetting"
    );
    assert!(run_ts(ts).is_empty(), "TS: callable boundary not resetting");
}

#[test]
fn loop_plus_else_if_cascade_is_two_levels() {
    let rs = r#"
fn f(items: &[i32]) {
    for &x in items {
        if x == 0 { a(); }
        else if x == 1 { b(); }
        else if x == 2 { c(); }
        else if x == 3 { d(); }
        else { e(); }
    }
}
"#;
    let ts = r#"
function f(items) {
    for (const x of items) {
        if (x === 0) a();
        else if (x === 1) b();
        else if (x === 2) c();
        else if (x === 3) d();
        else e();
    }
}
"#;
    assert!(run_rs(rs).is_empty());
    assert!(run_ts(ts).is_empty());
}
