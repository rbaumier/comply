//! rust-unsafe-ffi-isolation backend.
//!
//! Matches `extern "C"` / `extern "system"` blocks (`foreign_mod_item`)
//! and walks the ancestor chain looking for a `mod_item` named `sys`,
//! `ffi`, `raw`, or `bindings`. When no such wrapper exists, the FFI
//! block is exposed at the top of the file and gets flagged.

use crate::diagnostic::{Diagnostic, Severity};

const SAFE_MOD_NAMES: &[&str] = &["sys", "ffi", "raw", "bindings"];

crate::ast_check! { on ["foreign_mod_item"] => |node, source, ctx, diagnostics|
    let mut current = node.parent();
    while let Some(ancestor) = current {
        if ancestor.kind() == "mod_item"
            && let Some(name) = ancestor.child_by_field_name("name")
            && let Ok(text) = name.utf8_text(source)
            && SAFE_MOD_NAMES.contains(&text)
        {
            return;
        }
        current = ancestor.parent();
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &node,
        super::META.id,
        "Isolate `extern \"C\"` inside `mod sys { ... }` or `mod ffi { ... }`.".into(),
        Severity::Warning,
    ));
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
    use super::Check;
    use crate::diagnostic::Diagnostic;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.rs")
    }

    #[test]
    fn flags_extern_c_at_root() {
        assert_eq!(run(r#"extern "C" { fn foo(); }"#).len(), 1);
    }

    #[test]
    fn allows_extern_c_in_sys_mod() {
        assert!(run("mod sys {\n    extern \"C\" { fn foo(); }\n}").is_empty());
    }

    #[test]
    fn allows_extern_c_in_ffi_mod() {
        assert!(run("mod ffi {\n    extern \"C\" { fn bar(); }\n}").is_empty());
    }
}
