//! no-self-import oxc backend — flag a module that imports from itself.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use std::path::Path;
use std::sync::Arc;

pub struct Check;

/// Recognized JS/TS source extensions. Only these are stripped when deriving a
/// module name — companion suffixes like `.utils` or `.css` are part of the name.
const SOURCE_EXTENSIONS: &[&str] = &[
    "ts", "tsx", "mts", "cts", "js", "jsx", "mjs", "cjs",
];

/// Strip a single recognized source extension from a file name, leaving the
/// module name. `CodeBlock.tsx` → `CodeBlock`, `CodeBlock.utils` → `CodeBlock.utils`,
/// `App.css` → `App.css`.
fn module_name(name: &str) -> &str {
    if let Some((stem, ext)) = name.rsplit_once('.')
        && SOURCE_EXTENSIONS.contains(&ext)
    {
        return stem;
    }
    name
}

fn is_self_import(spec: &str, file_path: &Path) -> bool {
    let file_is_index = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s == "index");

    // `.` / `./` resolves to the directory's `index.*` barrel, not the file
    // itself — so it is only a self-import when the importing file IS that barrel.
    if spec == "." || spec == "./" {
        return file_is_index;
    }

    let stem = spec.trim_start_matches("./");
    if matches!(
        stem,
        "index" | "index.ts" | "index.tsx" | "index.js" | "index.jsx"
    ) && file_is_index
    {
        return true;
    }

    // A self-import resolves to the same file. Two relative siblings are the same
    // module only when their module names (file name minus a source extension)
    // are equal. Companion files (`Foo.utils`, `Foo.css`) keep their suffix in the
    // module name, so `Foo` ≠ `Foo.utils` — not a self-import.
    let Some(import_stem) = spec.strip_prefix("./") else {
        return false;
    };
    if import_stem.contains('/') {
        return false;
    }
    let Some(file_name) = file_path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };
    module_name(import_stem) == module_name(file_name)
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
        crate::rules::test_helpers::run_oxc_check(self, src, path, project, file)
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str, path: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, path)
    }

    #[test]
    fn allows_dot_import_in_non_index_file() {
        // `import from "."` in `src/utils/auth.ts` resolves to `src/utils/index.ts`,
        // a barrel — not a self-import.
        let src = "import { getEnvironmentVariable } from '.';\n";
        assert!(run(src, "src/utils/auth.ts").is_empty());
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
    fn allows_companion_module_files() {
        // `CodeBlock.tsx` importing `./CodeBlock.utils` / `.helpers` / `.types` —
        // distinct companion files, not self-imports.
        for spec in [
            "./CodeBlock.utils",
            "./CodeBlock.helpers",
            "./CodeBlock.types",
            "./CodeBlock.styles",
            "./CodeBlock.constants",
        ] {
            let src = format!("import {{ foo }} from '{spec}';\n");
            assert!(
                run(&src, "src/CodeBlock/CodeBlock.tsx").is_empty(),
                "{spec} should not be a self-import"
            );
        }
    }

    #[test]
    fn allows_css_module_import() {
        // `App.jsx` importing `./App.css` — different extension, different file.
        let src = "import './App.css';\n";
        assert!(run(src, "src/App.jsx").is_empty());
    }

    #[test]
    fn allows_compound_component_submodules() {
        // `Message.tsx` importing `./Message.Actions` — a separate sub-component file.
        for spec in ["./Message.Actions", "./Message.Context", "./Message.Display"] {
            let src = format!("import {{ foo }} from '{spec}';\n");
            assert!(
                run(&src, "src/Message/Message.tsx").is_empty(),
                "{spec} should not be a self-import"
            );
        }
    }

    #[test]
    fn flags_self_import_with_explicit_extension() {
        // True positive: a file importing its own path resolves to itself.
        let src = "import { foo } from './utils.ts';\n";
        let diags = run(src, "src/utils.ts");
        assert_eq!(diags.len(), 1);
        assert!(diags[0].message.contains("imports itself"));
    }
}
