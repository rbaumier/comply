use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::path::Path;

#[derive(Debug)]
pub struct Check;

/// Extract the module source from an import line.
fn extract_import_source(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if !trimmed.starts_with("import ") {
        return None;
    }
    let from_idx = trimmed.rfind(" from ")?;
    let rest = trimmed[from_idx + 6..].trim().trim_end_matches(';');
    let rest = rest.trim();
    if (rest.starts_with('\'') && rest.ends_with('\''))
        || (rest.starts_with('"') && rest.ends_with('"'))
    {
        Some(&rest[1..rest.len() - 1])
    } else {
        None
    }
}

/// Check if the import source refers to the file itself.
fn is_self_import(source: &str, file_path: &Path) -> bool {
    if source == "." || source == "./" {
        return true;
    }

    // ./index, ./index.ts, ./index.js, etc.
    let stem = source.trim_start_matches("./");
    if stem == "index"
        || stem == "index.ts"
        || stem == "index.tsx"
        || stem == "index.js"
        || stem == "index.jsx"
    {
        // Only flag if the file itself is an index file.
        if file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s == "index")
        {
            return true;
        }
    }

    // Check if the source matches the file's own name (e.g. `import x from './foo'` in `foo.ts`).
    if let Some(file_stem) = file_path.file_stem().and_then(|s| s.to_str()) {
        let import_stem = source.trim_start_matches("./");
        // Strip extension from import source if present.
        let import_base = Path::new(import_stem)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(import_stem);
        if import_base == file_stem && (source.starts_with("./") || source == ".") {
            return true;
        }
    }

    false
}

impl TextCheck for Check {
    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            if let Some(source) = extract_import_source(line)
                .filter(|s| is_self_import(s, ctx.path))
            {
                diagnostics.push(Diagnostic {
                    path: ctx.path.to_path_buf(),
                    line: idx + 1,
                    column: 1,
                    rule_id: "no-self-import".into(),
                    message: format!(
                        "Module imports itself (`{}`). Remove this import.",
                        source
                    ),
                    severity: Severity::Error,
                    span: None,
                });
            }
        }
        diagnostics
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    fn run(source: &str, path: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new(path), source))
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
}
