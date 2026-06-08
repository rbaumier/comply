//! Walks `attribute_item` nodes matching `#[allow(...)]`.
//! Flags when no comment exists on the same line, the line above, or the line below.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::rust_helpers::is_in_test_context;

crate::ast_check! { on ["attribute_item"] => |node, source, ctx, diagnostics|
    let text = node.utf8_text(source).unwrap_or("");
    if !text.starts_with("#[allow(") && !text.starts_with("#[allow (") {
        return;
    }

    if allow_list_contains(text, "unused") && is_in_test_context(node, source) {
        return;
    }

    let row = node.start_position().row;

    let src_str = std::str::from_utf8(source).unwrap_or("");
    let lines: Vec<&str> = src_str.lines().collect();

    if allow_list_contains(text, "dead_code") && has_adjacent_cfg_attribute(&lines, row) {
        return;
    }

    if allow_list_contains(text, "dead_code")
        && ctx.path.components().any(|c| c.as_os_str() == "tests")
    {
        return;
    }

    let has_inline_comment = lines.get(row).is_some_and(|l| {
        if let Some(pos) = l.find("//") {
            pos > l.find("#[allow").unwrap_or(0)
        } else {
            false
        }
    });

    let has_preceding_comment = row > 0 && lines.get(row - 1).is_some_and(|l| l.trim().starts_with("//"));
    let has_following_comment = lines.get(row + 1).is_some_and(|l| l.trim().starts_with("//"));

    if has_inline_comment || has_preceding_comment || has_following_comment {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        format!("`{text}` without justification — add a `//` comment explaining why."),
        Severity::Warning,
    ));
}

fn allow_list_contains(attribute: &str, name: &str) -> bool {
    let Some(start) = attribute.find('(') else {
        return false;
    };
    let Some(end) = attribute.rfind(')') else {
        return false;
    };
    attribute[start + 1..end].split(',').any(|part| {
        let candidate = part.trim();
        candidate == name || candidate.ends_with(&format!("::{name}"))
    })
}

fn has_adjacent_cfg_attribute(lines: &[&str], row: usize) -> bool {
    let prev_is_cfg = row > 0
        && lines
            .get(row - 1)
            .is_some_and(|line| line.trim_start().starts_with("#[cfg("));
    let next_is_cfg = lines
        .get(row + 1)
        .is_some_and(|line| line.trim_start().starts_with("#[cfg("));
    prev_is_cfg || next_is_cfg
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn flags_bare_allow() {
        assert_eq!(run("#[allow(dead_code)]\nfn f() {}").len(), 1);
    }

    #[test]
    fn allows_with_inline_comment() {
        assert!(run("#[allow(dead_code)] // kept for FFI compat\nfn f() {}").is_empty());
    }

    #[test]
    fn allows_with_preceding_comment() {
        assert!(
            run("// mirrors std API naming\n#[allow(clippy::wrong_self_convention)]\nfn f() {}")
                .is_empty()
        );
    }

    #[test]
    fn ignores_in_test_context() {
        assert!(run("#[cfg(test)]\nmod tests {\n#[allow(unused)]\nfn f() {}\n}").is_empty());
    }

    #[test]
    fn flags_dead_code_in_test_context_without_reason() {
        assert_eq!(
            run("#[cfg(test)]\nmod tests {\n#[allow(dead_code)]\nfn f() {}\n}").len(),
            1
        );
    }

    #[test]
    fn allows_dead_code_on_cfg_item() {
        assert!(run("#[cfg(feature = \"ffi\")]\n#[allow(dead_code)]\nfn f() {}").is_empty());
        assert!(run("#[allow(dead_code)]\n#[cfg(feature = \"ffi\")]\nfn f() {}").is_empty());
    }

    #[test]
    fn ignores_non_allow_attributes() {
        assert!(run("#[derive(Debug)]\nstruct S;").is_empty());
    }

    #[test]
    fn allows_with_following_comment() {
        assert!(run("#[allow(dead_code)]\n// justified below\ntype Foo = i32;").is_empty());
    }

    #[test]
    fn allows_dead_code_in_tests_dir() {
        assert!(crate::rules::test_helpers::run_rule(&Check, "#[allow(dead_code)]\ntype BoxStream<T> = Box<dyn Send>;", "tests/async_send_sync.rs")
        .is_empty());
    }
}
