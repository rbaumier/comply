use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::{has_attached_comment, is_test_code};

crate::ast_check! { on ["call_expression"] prefilter = ["Regex::new"] => |node, source, ctx, diagnostics|
    let Some(func_node) = node.child_by_field_name("function") else { return };
    let Ok(func_text) = func_node.utf8_text(source) else { return };
    if func_text != "Regex::new" { return; }

    if is_test_code(node, source, ctx) { return; }

    if is_in_named_binding_init(node) { return; }

    let Ok(text) = node.utf8_text(source) else { return };

    let pattern_len = extract_string_arg_len(text);
    if pattern_len < super::MIN_PATTERN_LEN { return; }

    if has_attached_comment(node) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Complex regex without a comment — add a description of what it matches.".into(),
        Severity::Error,
    ));
}

/// True when `node` is the initializer of a named `static_item` or `const_item`
/// binding — directly, or nested inside a `Lazy::new(|| …)` closure. In that
/// position the binding name (`START_SCRIPT`, `LF_TARGET_REGEX`, …) already
/// documents what the regex matches, so no separate comment is required.
///
/// Walks up the tree-sitter ancestor chain and returns `true` at the first
/// enclosing `static_item` / `const_item`; the walk stops at the source root.
fn is_in_named_binding_init(node: tree_sitter::Node) -> bool {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if matches!(parent.kind(), "static_item" | "const_item") {
            return true;
        }
        cur = parent;
    }
    false
}

fn extract_string_arg_len(call_text: &str) -> usize {
    let after_paren = match call_text.find('(') {
        Some(p) => &call_text[p + 1..],
        None => return 0,
    };

    if let Some(rest) = after_paren.strip_prefix("r\"") {
        return rest.find('"').unwrap_or(0);
    }
    if let Some(rest) = after_paren.strip_prefix("r#\"") {
        return rest.find("\"#").unwrap_or(0);
    }
    if after_paren.starts_with('"') {
        return after_paren[1..].find('"').unwrap_or(0);
    }
    0
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
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn flags_undocumented_complex_regex() {
        let src = "fn f() { let re = Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").unwrap(); }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_documented_regex() {
        let src = "fn f() {\n// ISO 8601 duration pattern\nlet re = Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").unwrap();\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_short_regex() {
        let src = "fn f() { let re = Regex::new(r\"^\\d+$\").unwrap(); }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_inline_comment() {
        let src = "fn f() { let re = Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").unwrap(); // duration\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_undocumented_regex_in_cfg_test_module() {
        let src = "#[cfg(test)]\nmod test {\nfn sanitize(s: String) -> String {\nlet re = Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").unwrap();\nre.replace_all(s.as_str(), \"x\").to_string()\n}\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_undocumented_regex_in_test_fn() {
        let src = "#[test]\nfn it_works() {\nlet re = Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").unwrap();\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_undocumented_regex_in_production_code() {
        let src = "fn parse() {\nlet re = Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").unwrap();\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_regex_in_named_static_lazy_init() {
        let src = "pub static START_SCRIPT: Lazy<Regex> =\n    Lazy::new(|| Regex::new(r#\"<script(?:.*type=\"(.*)\")?.*?>\"#).unwrap());";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_regex_in_named_const_init() {
        let src = "const LF_TARGET_REGEX: &str = Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").unwrap();";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_undocumented_let_bound_regex_in_fn() {
        let src = "fn f() {\nlet re = Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").unwrap();\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_trailing_block_comment() {
        // Regression for rbaumier/comply#6866 — starship/starship documents a
        // regex with a `/* … */` comment at the end of its own line.
        let src = concat!(
            "fn f() {\n",
            r##"let re = Regex::new(r#"(?m)^version( |\s*=\s*)['"](?P<version>[^'"]+)['"]$"#).unwrap(); /*dark magic*/"##,
            "\n}"
        );
        let diagnostics = run(src);
        assert!(diagnostics.is_empty(), "expected no diagnostics, got: {diagnostics:?}");
    }

    #[test]
    fn allows_block_comment_above() {
        let src = "fn f() {\n/* ISO 8601 duration pattern */\nlet re = Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").unwrap();\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_multi_line_block_comment_above() {
        let src = "fn f() {\n/* ISO 8601 duration:\n   years, months, days */\nlet re = Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").unwrap();\n}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_outer_doc_comment_above() {
        let src = "/// Matches an ISO 8601 duration.\npub fn f() -> bool { Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").is_ok() }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_inner_doc_comment_above() {
        let src = "//! Version parsing helpers.\npub fn f() -> bool { Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").is_ok() }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_line_comment_under_an_item_doc_comment() {
        let src = "/// Parses the manifest version.\n// tighten this pattern later\npub fn f() -> bool { Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").is_ok() }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_when_comment_documents_an_earlier_statement() {
        let src = "fn f() {\n// the counter starts at zero\nlet counter = 0;\nlet re = Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").unwrap();\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_pattern_containing_double_slash() {
        // `//` inside the pattern is part of the string literal, not a comment.
        let src = "fn f() {\nlet re = Regex::new(r\"^https?://[a-z0-9.-]+/(?P<path>.*)$\").unwrap();\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_when_previous_statement_carries_a_trailing_comment() {
        let src = "fn f(s: &str) {\nlet path = s.trim(); // strip surrounding blanks\nlet re = Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").unwrap();\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_when_previous_statement_carries_a_trailing_block_comment() {
        let src = "fn f(s: &str) {\nlet path = s.trim(); /* strip surrounding blanks */\nlet re = Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").unwrap();\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_when_a_comment_sits_in_an_enclosing_scope() {
        let src = "fn f(n: u8) {\nmatch n {\n0 => return, // nothing to parse for zero\n_ => {\nlet re = Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").unwrap();\n}\n}\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_when_a_blank_line_separates_the_comment() {
        let src = "fn f() {\n// ISO 8601 duration pattern\n\nlet re = Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").unwrap();\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_when_comment_annotates_a_later_argument() {
        let src = "fn f() {\nlet re = build(Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\"), /* case_insensitive */ true);\n}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn still_flags_when_the_opening_brace_carries_a_trailing_comment() {
        let src = "fn f() { // parses the version out of the manifest\nlet re = Regex::new(r\"^P(?:\\d+Y)?(?:\\d+M)?(?:\\d+D)?$\").unwrap();\n}";
        assert_eq!(run(src).len(), 1);
    }
}
