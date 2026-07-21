//! rust-mod-tests-without-cfg-test backend.
//!
//! Walks inline `mod_item` nodes whose name is `tests` (or `test`) — those
//! with a `declaration_list` body — and checks the preceding `attribute_item`
//! siblings for a `cfg` predicate that activates `test` — `#[cfg(test)]` and
//! compound forms like `#[cfg(all(test, …))]` / `#[cfg(any(test, …))]`. Flag if
//! absent. `#[cfg_attr(test, …)]` does not count: it applies another attribute
//! conditionally and leaves the module compiled in every build.
//!
//! A module named `test`/`tests` is only flagged when its body actually
//! contains a test-attributed function (`#[test]`, `#[tokio::test]`,
//! `#[rstest]`, `#[test_case]`, …) — that is what makes it a unit-test module
//! whose missing `#[cfg(test)]` gate would ship test code in release builds. A
//! `mod test`/`mod tests` holding only domain code (constants, types, runtime
//! logic) is a namespace that happens to be named `test`; gating it would
//! conditionally compile real code out of non-test builds, so it is not
//! flagged.
//!
//! Only inline `mod tests { … }` blocks are checked. An external declaration
//! `mod tests;` has no body to inspect, and the gate that matters for it is the
//! inner `#![cfg(test)]` of the referenced file (`tests.rs` / `tests/mod.rs`)
//! rather than anything on the declaration, so it is skipped.
//!
//! A module already gated on `cfg(test)` some other way is not flagged: the gate
//! can open the module's own body or an enclosing module's (`#![cfg(test)]`), sit
//! on an enclosing module or on the file, or sit on any `mod <name>;` declaration
//! along the chain that reaches this file from the crate root
//! (`ProjectCtx::rust_file_is_cfg_test_gated`). Adding a redundant `#[cfg(test)]`
//! there would change nothing about what ships.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{AstCheck, CheckCtx};
use crate::rules::rust_helpers::cfg_test_gates_compilation;

#[derive(Debug)]
pub struct Check;

impl AstCheck for Check {
    fn interested_kinds(&self) -> Option<&'static [&'static str]> {
        Some(&["mod_item"])
    }

    fn visit_node(
        &self,
        node: tree_sitter::Node,
        ctx: &CheckCtx,
        _state: Option<&mut dyn std::any::Any>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let source_bytes = ctx.source.as_bytes();
        let Some(name_node) = node.child_by_field_name("name") else {
            return;
        };
        let Ok(name) = name_node.utf8_text(source_bytes) else {
            return;
        };
        if name != "tests" && name != "test" {
            return;
        }
        // External declaration `mod tests;` has no `declaration_list` body — only
        // an inline `mod tests { … }` holds the test functions this rule looks
        // for.
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        // A `mod test`/`mod tests` is provably a unit-test module only when its
        // body holds a function carrying a test-runner attribute. Without one it
        // is a domain namespace that merely shares the name (issue #5638), and
        // forcing `#[cfg(test)]` would compile real code out of release builds.
        if !body_has_test_fn(body, source_bytes) {
            return;
        }
        // Nothing to add when the gate is already there: on the module, on the
        // scope compiling it, or on a `mod <name>;` along the chain of module
        // declarations reaching this file from the crate root — zellij's
        // `#[cfg(test)] mod unit;` gates every file under `unit/`, however deep.
        if cfg_test_gates_compilation(node, source_bytes)
            || ctx.project.rust_file_is_cfg_test_gated(ctx.path)
        {
            return;
        }
        let pos = node.start_position();
        diagnostics.push(Diagnostic {
            path: std::sync::Arc::clone(&ctx.path_arc),
            line: pos.row + 1,
            column: pos.column + 1,
            rule_id: "rust-mod-tests-without-cfg-test".into(),
            message: format!(
                "`mod {name}` is not gated by `#[cfg(test)]` — every \
                 test function will ship in the release binary. Add \
                 `#[cfg(test)]` immediately above the module declaration."
            ),
            severity: Severity::Error,
            span: None,
        });
    }
}

/// True if the module `body` (a `declaration_list`) directly contains a
/// function carrying a test-runner attribute. A `#[cfg(test)]` on a function is
/// not such a signal — it gates the function, it does not make it a test — so
/// this checks for genuine test attributes only, never the `cfg` forms that
/// `has_test_attribute` also accepts.
///
/// Only functions directly in the module body count: a test attribute on a
/// function nested inside a *child* module belongs to that child, not to this
/// module.
fn body_has_test_fn(body: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = body.walk();
    body.named_children(&mut cursor).any(|item| {
        item.kind() == "function_item" && fn_has_test_runner_attribute(item, source)
    })
}

/// True if `item` (a `function_item`) has a test-runner attribute as a
/// preceding `attribute_item` sibling: `#[test]`, a path test macro
/// (`#[tokio::test]`, `#[actix_rt::test(…)]`, …), or a framework test attribute
/// (`#[rstest]`, `#[test_case(…)]`, `#[proptest]`, …). Doc comments may
/// interleave the attributes; they are skipped, not treated as the end of the
/// attribute block.
fn fn_has_test_runner_attribute(item: tree_sitter::Node, source: &[u8]) -> bool {
    let mut sibling = item.prev_named_sibling();
    while let Some(s) = sibling {
        match s.kind() {
            "attribute_item" => {
                if let Ok(text) = s.utf8_text(source)
                    && attr_is_test_runner(text)
                {
                    return true;
                }
            }
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        sibling = s.prev_named_sibling();
    }
    false
}

/// True if a single attribute's source text is a test-runner attribute. Matches
/// the attribute path's last segment against the common test attributes so both
/// bare (`#[test]`) and path (`#[tokio::test]`) forms, with or without an
/// argument list, are recognized.
fn attr_is_test_runner(text: &str) -> bool {
    const TEST_ATTRS: &[&str] = &["test", "test_case", "rstest", "proptest"];
    // Strip the `#[` / `#![` framing and any argument list / trailing `]`, then
    // take the last `::`-delimited segment of the path.
    let inner = text
        .trim_start_matches("#![")
        .trim_start_matches("#[")
        .trim_start_matches('!')
        .trim();
    let path = inner.split(['(', ']']).next().unwrap_or(inner).trim();
    let last_segment = path.rsplit("::").next().unwrap_or(path).trim();
    TEST_ATTRS.contains(&last_segment)
}

#[cfg(test)]
impl crate::rules::test_helpers::RunRule for Check {
    fn meta(&self) -> &'static crate::rules::meta::RuleMeta {
        &super::META
    }
    fn execute_with_ctx(
        &self,
        src: &str,
        path: &std::path::Path,
        project: &crate::project::ProjectCtx,
        file: &crate::rules::file_ctx::FileCtx,
    ) -> Vec<crate::diagnostic::Diagnostic> {
        crate::rules::test_helpers::run_ast_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run_on(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.rs")
    }

    #[test]
    fn flags_mod_tests_without_cfg() {
        assert_eq!(run_on("mod tests { #[test] fn t() {} }").len(), 1);
    }

    #[test]
    fn does_not_flag_mod_test_domain_namespace() {
        // A `mod test` holding only domain code (constants, no test functions)
        // is a namespace, not a test module — gating it with `#[cfg(test)]`
        // would compile real code out of release builds (issue #5638).
        let source = "\
pub mod test {
    pub const FAILED: &str = \"test.failed\";
    pub const SETUP_FAILED: &str = \"test.setup_failed\";
    pub(crate) const ALL: &[&str] = &[FAILED, SETUP_FAILED];
}";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_mod_tests_with_only_types_and_fns() {
        // No test-attributed function anywhere in the body → domain namespace.
        let cases = [
            "mod tests { pub struct Foo; pub fn build() -> Foo { Foo } }",
            "mod test { type Code = u32; const X: Code = 1; }",
            "mod tests { pub use crate::foo::Bar; }",
        ];
        for source in cases {
            assert!(
                run_on(source).is_empty(),
                "should not flag namespace with no test fn: {source}"
            );
        }
    }

    #[test]
    fn does_not_flag_when_only_child_module_has_test_fn() {
        // The test attribute is on a function nested in a *child* module; the
        // outer `mod tests` itself holds no test function, so it is a namespace.
        let source = "mod tests { mod inner { #[test] fn t() {} } }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn flags_mod_tests_with_framework_test_fn() {
        // Framework test attributes mark a real test module just as `#[test]`
        // does, so an ungated module containing one is still flagged.
        let cases = [
            "mod tests { #[tokio::test] async fn t() {} }",
            "mod tests { #[rstest] fn t() {} }",
            "mod test { #[test_case(1)] fn t(_x: u32) {} }",
        ];
        for source in cases {
            assert_eq!(
                run_on(source).len(),
                1,
                "should flag ungated module with a framework test fn: {source}"
            );
        }
    }

    #[test]
    fn does_not_flag_external_mod_tests_declaration() {
        // `mod tests;` references a file gated by an inner `#![cfg(test)]`,
        // not visible here, so the external declaration must not be flagged
        // (issue #3787).
        assert!(run_on("mod tests;").is_empty());
    }

    #[test]
    fn does_not_flag_external_mod_test_declaration() {
        assert!(run_on("mod test;").is_empty());
    }

    #[test]
    fn allows_mod_tests_with_cfg_test() {
        let source = "#[cfg(test)]\nmod tests { #[test] fn t() {} }";
        assert!(run_on(source).is_empty());
    }

    #[test]
    fn does_not_flag_other_module() {
        assert!(run_on("mod helpers { fn h() {} }").is_empty());
    }

    #[test]
    fn allows_mod_tests_with_cfg_test_and_interleaved_doc_comment() {
        // A doc comment between `#[cfg(test)]` and `mod tests` must not break
        // attribute detection — attributes and doc comments may interleave in
        // any order (issue #4496).
        let cases = [
            "#[cfg(test)]\n/// Tests for the parser.\nmod tests { use super::*; }",
            "#[cfg(test)]\n/// line 1\n/// line 2\nmod tests { fn t() {} }",
            "#[cfg(test)]\n/** doc */\nmod tests { fn t() {} }",
        ];
        for source in cases {
            assert!(
                run_on(source).is_empty(),
                "should not flag cfg(test) split by a doc comment: {source}"
            );
        }
    }

    #[test]
    fn still_flags_doc_comment_without_cfg_test() {
        // A doc comment on `mod tests` with no `#[cfg(test)]` at all must stay
        // flagged — traversing comments must not invent a missing attribute.
        assert_eq!(run_on("/// docs\nmod tests { #[test] fn t() {} }").len(), 1);
    }

    #[test]
    fn allows_mod_tests_with_compound_cfg_test() {
        let cases = [
            "#[cfg(all(test, not(loom)))]\nmod tests { #[test] fn t() {} }",
            "#[cfg(any(test, feature = \"x\"))]\nmod tests { #[test] fn t() {} }",
            "#[cfg(all(test, target_has_atomic = \"64\"))]\nmod test { #[test] fn t() {} }",
        ];
        for source in cases {
            assert!(
                run_on(source).is_empty(),
                "should not flag compound cfg(test): {source}"
            );
        }
    }

    #[test]
    fn skips_cargo_integration_test_file() {
        // Under a `tests/` dir the whole file is compiled test-only — an inner
        // `mod tests` without `#[cfg(test)]` must not be flagged (issue #1325).
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            "mod tests { #[test] fn t() {} }",
            "tests/integration.rs",
        );
        assert!(diags.is_empty(), "must not flag mod tests in a tests/ integration file");
    }

    #[test]
    fn still_flags_mod_tests_in_src_file() {
        // A regular `src/*.rs` file is compiled into the production binary, so a
        // `mod tests` there genuinely needs `#[cfg(test)]` and stays flagged.
        let diags = crate::rules::test_helpers::run_rule_gated(
            &Check,
            "mod tests { #[test] fn t() {} }",
            "src/lib.rs",
        );
        assert_eq!(diags.len(), 1, "must still flag mod tests in a src/ file");
    }

    /// Stage a crate on disk and run the rule on `target_rel`, so the
    /// module-declaration chain that reaches it is resolvable off disk.
    fn run_in_crate(files: &[(&str, &str)], target_rel: &str, target_src: &str) -> Vec<Diagnostic> {
        use std::fs;
        use tempfile::TempDir;
        let dir = TempDir::new().unwrap();
        let target_path = dir.path().join(target_rel);
        for (rel, src) in files {
            let path = dir.path().join(rel);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, src).unwrap();
        }
        fs::create_dir_all(target_path.parent().unwrap()).unwrap();
        fs::write(&target_path, target_src).unwrap();
        crate::rules::test_helpers::run_rule_gated(&Check, target_src, &target_path)
    }

    #[test]
    fn does_not_flag_mod_tests_in_file_included_under_cfg_test() {
        // `remote_attach/mod.rs` pulls the whole `unit/` directory in behind
        // `#[cfg(test)]`, so nothing under it ships in a release binary and the
        // inner `mod tests` blocks need no redundant gate (issue #6815).
        let source = "\
#[cfg(feature = \"web_server_capability\")]
mod tests {
    #[tokio::test]
    async fn test_attach() {}
}

#[cfg(not(feature = \"web_server_capability\"))]
mod tests {
    #[test]
    fn test_url_parsing() {}
}";
        let diags = run_in_crate(
            &[
                ("src/lib.rs", "mod remote_attach;\n"),
                ("src/remote_attach/mod.rs", "#[cfg(test)]\nmod unit;\n"),
                (
                    "src/remote_attach/unit/mod.rs",
                    "mod remote_attach_tests;\n",
                ),
            ],
            "src/remote_attach/unit/remote_attach_tests.rs",
            source,
        );
        assert!(
            diags.is_empty(),
            "must not flag mod tests in a file the parent module includes under #[cfg(test)]"
        );
    }

    #[test]
    fn still_flags_mod_tests_in_file_included_unconditionally() {
        // Same layout, no `#[cfg(test)]` on the `mod unit;` declaration: the file
        // is compiled into the release binary, so the gate is missing.
        let diags = run_in_crate(
            &[
                ("src/lib.rs", "mod remote_attach;\n"),
                ("src/remote_attach/mod.rs", "mod unit;\n"),
                (
                    "src/remote_attach/unit/mod.rs",
                    "mod remote_attach_tests;\n",
                ),
            ],
            "src/remote_attach/unit/remote_attach_tests.rs",
            "mod tests { #[test] fn t() {} }",
        );
        assert_eq!(
            diags.len(),
            1,
            "must still flag mod tests in a file that ships in the release binary"
        );
    }

    #[test]
    fn does_not_flag_mod_tests_in_cfg_test_file_or_module() {
        // The compilation scope is already `cfg(test)` inside the file itself:
        // a `#![cfg(test)]` file attribute, an enclosing gated module, or the
        // module's own body opening with `#![cfg(test)]`.
        let cases = [
            "#![cfg(test)]\nmod tests { #[test] fn t() {} }",
            "#[cfg(test)]\nmod harness { mod tests { #[test] fn t() {} } }",
            "mod tests { #![cfg(test)]\n#[test] fn t() {} }",
            "mod harness { #![cfg(test)]\nmod tests { #[test] fn t() {} } }",
        ];
        for source in cases {
            assert!(
                run_on(source).is_empty(),
                "should not flag a module already compiled under cfg(test): {source}"
            );
        }
    }

    #[test]
    fn still_flags_mod_tests_under_a_cfg_attr_test_scope() {
        // `#[cfg_attr(test, …)]` applies another attribute conditionally and
        // leaves the item compiled in every build, so it gates nothing: the
        // `mod tests` still ships and still needs `#[cfg(test)]`.
        let cases = [
            // Directly on the module — the form found in real code.
            "#[cfg_attr(test, allow(dead_code))]\nmod tests { #[test] fn t() {} }",
            "#![cfg_attr(test, allow(dead_code))]\nmod tests { #[test] fn t() {} }",
            "#[cfg_attr(test, allow(dead_code))]\nmod harness { mod tests { #[test] fn t() {} } }",
        ];
        for source in cases {
            assert_eq!(
                run_on(source).len(),
                1,
                "cfg_attr is not a compilation gate: {source}"
            );
        }
    }

    #[test]
    fn still_flags_mod_tests_with_non_test_cfg() {
        let cases = [
            "mod tests { #[test] fn t() {} }",
            "#[cfg(feature = \"x\")]\nmod tests { #[test] fn t() {} }",
            "#[cfg(not(test))]\nmod tests { #[test] fn t() {} }",
        ];
        for source in cases {
            assert_eq!(
                run_on(source).len(),
                1,
                "should still flag non-test-cfg mod tests: {source}"
            );
        }
    }
}
