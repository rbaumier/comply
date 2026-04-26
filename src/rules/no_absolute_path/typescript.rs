//! no-absolute-path backend — flag `import x from '/foo'` where the module
//! specifier starts with `/` (filesystem absolute path).

use crate::diagnostic::{Diagnostic, Severity};

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "import_statement" { return; }

    let Some(src_node) = node.child_by_field_name("source") else { return };
    let raw = src_node.utf8_text(source).unwrap_or("");
    let spec = raw.trim_matches(|c: char| c == '\'' || c == '"' || c == '`');
    if !spec.starts_with('/') { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &src_node,
        super::META.id,
        format!("Do not import modules using an absolute path (`{spec}`)."),
        Severity::Warning,
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_ts(source, &Check)
    }

    #[test]
    fn flags_absolute_import() {
        let src = "import { foo } from '/usr/lib/utils';\n";
        let diags = run(src);
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("absolute path"));
    }

    #[test]
    fn allows_relative_import() {
        let src = "import { foo } from './utils';\n";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_package_import() {
        let src = "import { foo } from 'lodash';\n";
        assert!(run(src).is_empty());
    }
}
