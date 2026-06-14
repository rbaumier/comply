//! rust-unsafe-ffi-isolation backend.
//!
//! Matches `extern "C"` / `extern "system"` blocks (`foreign_mod_item`)
//! whose ABI is a genuine foreign calling convention, then walks the
//! ancestor chain looking for a `mod_item` named `sys`, `ffi`, `raw`,
//! or `bindings`. When no such wrapper exists, the FFI block is exposed
//! at the top of the file and gets flagged.
//!
//! A `foreign_mod_item` with a non-foreign ABI string (e.g.
//! `extern "SQL"`, a DSL marker for diesel's `#[declare_sql_function]`
//! proc macro) is not a C/foreign interface and is left untouched.

use crate::diagnostic::{Diagnostic, Severity};

const SAFE_MOD_NAMES: &[&str] = &["sys", "ffi", "raw", "bindings"];

/// Genuine foreign calling conventions accepted by rustc. A bare
/// `extern { ... }` carries no ABI string and implicitly means `"C"`,
/// so it is treated as foreign too. Any other ABI string (`"SQL"`,
/// `"Rust"`, …) is a DSL marker or the native ABI, not foreign FFI.
const FOREIGN_ABIS: &[&str] = &[
    "C",
    "C-unwind",
    "system",
    "system-unwind",
    "cdecl",
    "cdecl-unwind",
    "stdcall",
    "stdcall-unwind",
    "fastcall",
    "fastcall-unwind",
    "thiscall",
    "thiscall-unwind",
    "vectorcall",
    "vectorcall-unwind",
    "win64",
    "win64-unwind",
    "sysv64",
    "sysv64-unwind",
    "aapcs",
    "aapcs-unwind",
    "efiapi",
];

/// Reads the ABI string of a `foreign_mod_item` from its
/// `extern_modifier`. Returns `None` for a bare `extern { ... }`, which
/// implicitly uses the `"C"` ABI.
fn abi<'a>(node: &tree_sitter::Node, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    let modifier = node
        .children(&mut cursor)
        .find(|child| child.kind() == "extern_modifier")?;
    let mut modifier_cursor = modifier.walk();
    let literal = modifier
        .children(&mut modifier_cursor)
        .find(|child| child.kind() == "string_literal")?;
    let mut c = literal.walk();
    let content = literal
        .children(&mut c)
        .find(|child| child.kind() == "string_content")?;
    content.utf8_text(source).ok()
}

crate::ast_check! { on ["foreign_mod_item"] => |node, source, ctx, diagnostics|
    // A bare `extern { ... }` (no ABI string) is implicitly `"C"` FFI.
    if let Some(abi) = abi(&node, source)
        && !FOREIGN_ABIS.contains(&abi)
    {
        return;
    }

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

    #[test]
    fn flags_bare_extern_block() {
        // A bare `extern { ... }` is implicitly the `"C"` ABI.
        assert_eq!(run(r#"extern { fn foo(); }"#).len(), 1);
    }

    #[test]
    fn allows_extern_sql_dsl_marker() {
        // diesel's `#[declare_sql_function]` uses `extern "SQL"` as a DSL
        // marker, not a foreign function interface.
        let src = "#[crate::declare_sql_function]\n\
                   extern \"SQL\" {\n    fn lower(x: VarChar) -> VarChar;\n}";
        assert!(run(src).is_empty());
    }
}
