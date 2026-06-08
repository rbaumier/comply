//! Fires once per file when the file contains both a decorator node and
//! an import of `reflect-metadata` (the canonical marker for the
//! legacy experimentalDecorators system).

use crate::diagnostic::{Diagnostic, Severity};

fn has_decorator(node: tree_sitter::Node) -> bool {
    if node.kind() == "decorator" {
        return true;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if has_decorator(child) {
            return true;
        }
    }
    false
}

fn has_reflect_metadata_import(program: tree_sitter::Node, source: &[u8]) -> bool {
    let mut cursor = program.walk();
    for child in program.named_children(&mut cursor) {
        if child.kind() == "import_statement" {
            let text = std::str::from_utf8(&source[child.byte_range()]).unwrap_or("");
            if text.contains("'reflect-metadata'") || text.contains("\"reflect-metadata\"") {
                return true;
            }
        }
    }
    false
}

fn first_decorator_node(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    if node.kind() == "decorator" {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(n) = first_decorator_node(child) {
            return Some(n);
        }
    }
    None
}

crate::ast_check! { on ["program"] => |node, source, ctx, diagnostics|
    if !has_decorator(node) {
        return;
    }
    if !has_reflect_metadata_import(node, source) {
        return;
    }
    let Some(dec) = first_decorator_node(node) else { return };
    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &dec,
        super::META.id,
        "File mixes decorators with a `reflect-metadata` import — standard and experimental decorator systems cannot coexist.".into(),
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
    fn flags_mixed_decorators_with_reflect_metadata() {
        let src = "import 'reflect-metadata';\n@Injectable() class Svc {}";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_decorators_without_reflect_metadata() {
        let src = "@Injectable() class Svc {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_reflect_metadata_without_decorators() {
        let src = "import 'reflect-metadata';\nconst x = 1;";
        assert!(run(src).is_empty());
    }
}
