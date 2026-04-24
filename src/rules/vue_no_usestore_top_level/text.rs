//! vue-no-usestore-top-level AST backend.

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

fn top_level_line(body: &str, line_off: usize) -> bool {
    let mut depth = 0i32;
    let mut cur_line = 0usize;
    for b in body.bytes() {
        if cur_line == line_off {
            break;
        }
        match b {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b'\n' => cur_line += 1,
            _ => {}
        }
    }
    depth == 0
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "component" { return; }
    let _ = source;
    let src = ctx.source;
    let Some((start, end, base_line)) = find_define_store_body(src) else {
        return;
    };
    let body = &src[start..end];
    for (idx, line) in body.lines().enumerate() {
        let trimmed = line.trim_start();
        if !(trimmed.starts_with("const ") || trimmed.starts_with("let ") || trimmed.starts_with("var ")) {
            continue;
        }
        let after_eq = trimmed.split_once('=').map(|(_, r)| r.trim_start()).unwrap_or("");
        let mut found = None;
        let chars = after_eq.char_indices().peekable();
        for (i, c) in chars {
            if c == 'u' && after_eq[i..].starts_with("use") {
                let rest = &after_eq[i + 3..];
                let len: usize = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric())
                    .map(|c| c.len_utf8())
                    .sum();
                let ident = &rest[..len];
                if !ident.is_empty()
                    && ident.chars().next().unwrap().is_ascii_uppercase()
                    && ident.ends_with("Store")
                    && rest[len..].starts_with('(')
                {
                    found = Some(format!("use{ident}"));
                    break;
                }
            }
        }
        if let Some(name) = found
            && top_level_line(body, idx)
        {
            diagnostics.push(Diagnostic {
                path: ctx.path.to_path_buf(),
                line: base_line + idx + 1,
                column: 1,
                rule_id: super::META.id.into(),
                message: format!(
                    "`{name}()` at the top of a store setup pins Pinia init order — move it inside an action or getter."
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
    fn flags_usestore_top_level() {
        let sfc = "<script setup>\nexport const useA = defineStore('a', () => {\n  const other = useOtherStore()\n  return {}\n})\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_usestore_inside_action() {
        let sfc = "<script setup>\nexport const useA = defineStore('a', () => {\n  const load = () => {\n    const other = useOtherStore()\n    return other\n  }\n  return { load }\n})\n</script>";
        assert!(run(sfc).is_empty());
    }
}
