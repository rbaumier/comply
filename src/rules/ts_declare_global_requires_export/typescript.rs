//! Fires once per file: if an `ambient_declaration` names `global` and the
//! file has no `export_statement` or `import_statement`, flag the
//! `declare global` node.

use crate::diagnostic::{Diagnostic, Severity};

fn is_declare_global(node: tree_sitter::Node, source: &[u8]) -> bool {
    if node.kind() != "ambient_declaration" {
        return false;
    }
    let text = std::str::from_utf8(&source[node.byte_range()]).unwrap_or("");
    text.starts_with("declare global")
}

fn program_has_module_marker(program: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = program.walk();
    for child in program.named_children(&mut cursor) {
        match child.kind() {
            "export_statement" => return true,
            "import_statement" => {
                // A side-effect import counts as module marker.
                let text = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
                if !text.trim().is_empty() {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    let mut cursor = node.walk();
    let declare_global_node: Option<tree_sitter::Node> = node
        .named_children(&mut cursor)
        .find(|c| is_declare_global(*c, source));

    let Some(dg) = declare_global_node else { return };
    if program_has_module_marker(node, source) {
        return;
    }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &dg,
        super::META.id,
        "`declare global` only works in module files — add `export {};` to the file.".into(),
        Severity::Error,
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
    use super::*;

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "t.ts")
    }

    #[test]
    fn flags_declare_global_without_export() {
        let src = "declare global { interface Window { foo: string; } }";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_with_export_empty() {
        let src = "declare global { interface Window { foo: string; } }\nexport {};";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_with_import() {
        let src = "import './side-effect';\ndeclare global { interface Window { foo: string; } }";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_file_without_declare_global() {
        let src = "const x = 1;";
        assert!(run(src).is_empty());
    }
}
