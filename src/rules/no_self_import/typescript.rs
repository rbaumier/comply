//! no-self-import backend — flag a module that imports from itself.

use crate::diagnostic::{Diagnostic, Severity};
use std::path::Path;

/// Check if the import source refers to the file itself.
fn is_self_import(spec: &str, file_path: &Path) -> bool {
    if spec == "." || spec == "./" {
        return true;
    }

    // ./index, ./index.ts, ./index.js, etc.
    let stem = spec.trim_start_matches("./");
    if matches!(
        stem,
        "index" | "index.ts" | "index.tsx" | "index.js" | "index.jsx"
    ) && file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s == "index")
    {
        return true;
    }

    // Check if the source matches the file's own name (e.g. `import x from './foo'` in `foo.ts`).
    if let Some(file_stem) = file_path.file_stem().and_then(|s| s.to_str()) {
        let import_stem = spec.trim_start_matches("./");
        if !import_stem.contains('/') {
            let import_base = Path::new(import_stem)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(import_stem);
            if import_base == file_stem && spec.starts_with("./") {
                return true;
            }
        }
    }

    false
}

crate::ast_check! { on ["import_statement"] => |node, source, ctx, diagnostics|
    let Some(src_node) = node.child_by_field_name("source") else { return };
    let raw = src_node.utf8_text(source).unwrap_or("");
    let spec = raw.trim_matches(|c: char| c == '\'' || c == '"' || c == '`');
    if !is_self_import(spec, ctx.path) { return; }

    diagnostics.push(Diagnostic::at_node(
        ctx.path,
        &src_node,
        super::META.id,
        format!("Module imports itself (`{spec}`). Remove this import."),
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

    fn run(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn flags_dot_import_in_index() {
        let src = "import { foo } from '.';\n";
        let diags = run(src, "src/index.ts");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("imports itself"));
    }

    #[test]
    fn flags_self_name_import() {
        let src = "import { foo } from './utils';\n";
        let diags = run(src, "src/utils.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_different_module() {
        let src = "import { foo } from './other';\n";
        assert!(run(src, "src/utils.ts").is_empty());
    }

    #[test]
    fn flags_index_import_in_index_file() {
        let src = "import { foo } from './index';\n";
        let diags = run(src, "src/index.ts");
        assert_eq!(diags.len(), 1);
    }

    #[test]
    fn allows_subdir_index_import_from_index() {
        let src = "import { foo } from './_lib/formatDistance/index.ts';\n";
        assert!(run(src, "src/locale/sl/index.ts").is_empty());
    }
}
