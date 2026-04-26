//! vue-setup-store-return-all AST backend.
//!
//! Scans a `defineStore('name', () => { ... })` block, collects names bound to
//! `ref(`, `reactive(`, `computed(`, then checks the trailing `return { ... }`
//! includes each name.

use crate::diagnostic::{Diagnostic, Severity};

fn find_define_store_body(src: &str) -> Option<(usize, usize, usize)> {
    let pos = src.find("defineStore(")?;
    let after = &src[pos..];
    let brace_open = after.find('{')?;
    let abs_open = pos + brace_open;
    let bytes = src.as_bytes();
    let mut depth = 1i32;
    let mut j = abs_open + 1;
    while j < bytes.len() && depth > 0 {
        match bytes[j] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        j += 1;
    }
    let line = src[..abs_open].matches('\n').count();
    Some((abs_open + 1, j.saturating_sub(1), line))
}

crate::ast_check! { on ["component"] => |node, source, ctx, diagnostics|
    let _ = source;
    let src = ctx.source;
    let Some((start, end, base_line)) = find_define_store_body(src) else {
        return;
    };
    let body = &src[start..end];
    let mut names: Vec<String> = Vec::new();
    for line in body.lines() {
        let trimmed = line.trim_start();
        for prefix in ["const ", "let ", "var "] {
            if let Some(rest) = trimmed.strip_prefix(prefix) {
                let Some(eq) = rest.find('=') else { continue };
                let name = rest[..eq].trim().trim_end_matches(':');
                let after = rest[eq + 1..].trim_start();
                if (after.starts_with("ref(")
                    || after.starts_with("shallowRef(")
                    || after.starts_with("reactive(")
                    || after.starts_with("computed("))
                    && !name.is_empty()
                    && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                {
                    names.push(name.to_string());
                }
            }
        }
    }
    if names.is_empty() {
        return;
    }
    let Some(ret_pos) = body.rfind("return ") else {
        return;
    };
    let ret_body = &body[ret_pos..];
    for name in &names {
        let in_return = ret_body.contains(&format!("{name},"))
            || ret_body.contains(&format!("{name} "))
            || ret_body.contains(&format!("{name}\n"))
            || ret_body.contains(&format!("{name}}}"))
            || ret_body.contains(&format!("{name}:"));
        if !in_return {
            let mut decl_line = base_line;
            for (off, line) in body.lines().enumerate() {
                if line.contains(&format!("const {name} ")) || line.contains(&format!("const {name}=")) {
                    decl_line = base_line + off + 1;
                    break;
                }
            }
            diagnostics.push(Diagnostic {
                path: std::sync::Arc::clone(&ctx.path_arc),
                line: decl_line,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "Pinia setup store does not return `{name}` — the binding will be inaccessible outside the store."
                ),
                severity: Severity::Error,
                span: None,
            });
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
    fn flags_missing_return() {
        let sfc = "<script setup>\nexport const useX = defineStore('x', () => {\n  const count = ref(0)\n  const name = ref('')\n  return { count }\n})\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_complete_return() {
        let sfc = "<script setup>\nexport const useX = defineStore('x', () => {\n  const count = ref(0)\n  return { count }\n})\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn ignores_non_store() {
        let sfc = "<script setup>\nconst f = () => { const x = ref(0); return null }\n</script>";
        assert!(run(sfc).is_empty());
    }
}
