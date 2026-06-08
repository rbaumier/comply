//! no-self-import oxc backend — flag a module that imports from itself.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::path::Path;
use std::sync::Arc;

pub struct Check;

fn is_self_import(spec: &str, file_path: &Path) -> bool {
    if spec == "." || spec == "./" {
        return true;
    }

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

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ImportDeclaration]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ImportDeclaration(import) = node.kind() else { return };
        let spec = import.source.value.as_str();
        if !is_self_import(spec, ctx.path) {
            return;
        }

        let (line, column) =
            byte_offset_to_line_col(ctx.source, import.source.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!("Module imports itself (`{spec}`). Remove this import."),
            severity: Severity::Error,
            span: None,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;



    fn run(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts_with_path(source, &Check, path)
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
