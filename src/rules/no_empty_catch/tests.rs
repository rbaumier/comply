use super::oxc_typescript::Check;
use crate::rules::test_helpers::{run_rule, run_rule_gated};

#[test]
fn flags_discarded_silent_rejection_handlers() {
    for src in [
        "p.catch(() => undefined);",
        "p.catch(() => null);",
        "void p.catch(() => void 0);",
        "p.then(ok, () => null);",
    ] {
        let diagnostics = run_rule(&Check, src, "a.ts");
        assert_eq!(diagnostics.len(), 1, "{src}: {diagnostics:?}");
        assert_eq!(diagnostics[0].rule_id, "no-empty-catch", "{src}");
    }
}

#[test]
fn ignores_value_position_used_error_empty_body_and_comment() {
    for src in [
        "async function f() { const data = await r.json().catch(() => null); }",
        "function f() { return p.catch(() => null); }",
        "use(p.catch(() => null));",
        "p.catch((e) => { log(e); return null });",
        "p.catch((e) => void report(e));",
        "p.catch(() => {});",
        "p.catch(() => /* best effort */ undefined);",
    ] {
        let diagnostics = run_rule(&Check, src, "a.ts");
        assert!(diagnostics.is_empty(), "{src}: {diagnostics:?}");
    }
}

#[test]
fn skips_silent_handler_in_test_file_but_not_empty_catch_block() {
    let diagnostics = run_rule_gated(
        &Check,
        "p.catch(() => undefined);\ntry { f(); } catch {}\n",
        "src/a.test.ts",
    );
    assert_eq!(diagnostics.len(), 1, "{diagnostics:?}");
    assert_eq!(diagnostics[0].line, 2);
}
