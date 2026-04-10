//! Shared helper for matching `<test_base>.<method>(...)` patterns in
//! tree-sitter ASTs. Used by `no-focused-test` (`.only`) and
//! `no-skipped-test-without-link` (`.skip`).
//!
//! Both rules walk `member_expression` nodes, extract the `object` and
//! `property` fields, check the object name against a closed list of
//! test framework entry points (`test`, `it`, `describe`, …), and then
//! decide whether the property name is the one they care about. Without
//! this module they each carried a copy of the same 12-line walker.

/// Test framework entry points whose `.only` / `.skip` / `.todo`
/// methods are recognised. Closed list — adding a new framework requires
/// editing this constant.
pub const TEST_BASES: &[&str] = &["test", "it", "describe", "suite", "context"];

/// One matched `<test_base>.<method>` member expression.
#[derive(Debug)]
pub struct TestMethodMatch<'a> {
    /// The test framework base name (`test`, `it`, …).
    pub base: &'a str,
    /// The method called on it (`only`, `skip`, `todo`, …). Callers
    /// then filter by the specific method they care about.
    pub method: &'a str,
}

/// Try to match `node` as a `<test_base>.<method>` member expression
/// where the base is one of the recognised test entry points. Returns
/// `None` for any other node shape.
#[must_use]
pub fn match_test_member_call<'a>(
    node: tree_sitter::Node,
    source: &'a [u8],
) -> Option<TestMethodMatch<'a>> {
    if node.kind() != "member_expression" {
        return None;
    }
    let object = node.child_by_field_name("object")?;
    let property = node.child_by_field_name("property")?;
    let base = object.utf8_text(source).ok()?;
    let method = property.utf8_text(source).ok()?;
    if !TEST_BASES.contains(&base) {
        return None;
    }
    Some(TestMethodMatch { base, method })
}
