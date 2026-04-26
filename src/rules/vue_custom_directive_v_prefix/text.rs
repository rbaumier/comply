//! vue-custom-directive-v-prefix AST backend.
//!
//! Looks for template usage `v-foo` where `foo` is used as a directive, then
//! checks that the matching `<script setup>` identifier declared with object
//! `{ mounted, updated, ... }` shape is named `vFoo`.
//!
//! Simpler heuristic: if the template uses a directive `v-<local>` and the
//! script has a matching binding called exactly `<local>` (lowercase, no `v`
//! prefix) with directive-hook keys in its value, flag it.

use crate::diagnostic::{Diagnostic, Severity};

fn looks_like_directive_object(line: &str) -> bool {
    line.contains('{')
        && (line.contains("mounted")
            || line.contains("updated")
            || line.contains("beforeMount")
            || line.contains("beforeUpdate")
            || line.contains("unmounted")
            || line.contains("beforeUnmount"))
}

crate::ast_check! { on ["component"] => |node, source, ctx, diagnostics|
    let _ = source;
    for (idx, line) in ctx.source.lines().enumerate() {
        let trimmed = line.trim_start();
        for prefix in ["const ", "let ", "var "] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                let Some(eq) = rest.find('=') else { continue };
                let name = rest[..eq].trim().trim_end_matches(':');
                if name.is_empty()
                    || !name.chars().all(|c| c.is_alphanumeric() || c == '_')
                {
                    continue;
                }
                if !looks_like_directive_object(rest) {
                    continue;
                }
                let ok = name.starts_with('v')
                    && name.len() > 1
                    && name.as_bytes()[1].is_ascii_uppercase();
                if ok {
                    continue;
                }
                let dir = format!("v-{name}");
                if ctx.source.contains(&dir) {
                    diagnostics.push(Diagnostic {
                        path: ctx.path.to_path_buf(),
                        line: idx + 1,
                        column: 1,
                        rule_id: super::META.id.into(),
                        message: format!(
                            "Local directive `{name}` must be named `v{}{}` to be recognized in `<script setup>`.",
                            name.chars().next().unwrap_or('X').to_ascii_uppercase(),
                            &name[name.chars().next().map(|c| c.len_utf8()).unwrap_or(0)..]
                        ),
                        severity: Severity::Error,
                        span: None,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::backend::{AstCheck, CheckCtx};
    use std::path::Path;

    fn run(source: &str) -> Vec<Diagnostic> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_vue_updated::language())
            .expect("vue grammar");
        let tree = parser.parse(source, None).expect("parser");
        Check.check(&CheckCtx::for_test(Path::new("t.vue"), source), &tree)
    }

    #[test]
    fn flags_lowercase_directive() {
        let sfc = "<script setup>\nconst focus = { mounted: (el) => el.focus() }\n</script>\n<template>\n<input v-focus />\n</template>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_v_prefix() {
        let sfc = "<script setup>\nconst vFocus = { mounted: (el) => el.focus() }\n</script>\n<template>\n<input v-focus />\n</template>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn ignores_non_directive_object() {
        let sfc = "<script setup>\nconst config = { foo: 1 }\n</script>";
        assert!(run(sfc).is_empty());
    }
}
