//! vue-no-ssr-globals-in-setup AST backend.

use crate::diagnostic::{Diagnostic, Severity};

const SSR_GLOBALS: &[&str] = &["window", "document", "localStorage", "sessionStorage", "navigator"];

fn script_setup_range(source: &str) -> Option<(usize, usize)> {
    for (i, _) in source.match_indices("<script") {
        let close = source[i..].find('>')?;
        let tag = &source[i..i + close];
        if tag.contains("setup") {
            let body_start = i + close + 1;
            let end_rel = source[body_start..].find("</script>")?;
            return Some((body_start, body_start + end_rel));
        }
    }
    None
}

crate::ast_check! { |node, source, ctx, diagnostics|
    if node.kind() != "component" { return; }
    let _ = source;
    let Some((start, end)) = script_setup_range(ctx.source) else {
        return;
    };
    let body = &ctx.source[start..end];
    let base_line = ctx.source[..start].matches('\n').count();
    let mut depth = 0i32;
    for (idx, line) in body.lines().enumerate() {
        let trimmed_line = line.trim();
        if trimmed_line.starts_with("//") {
            continue;
        }
        if depth == 0 {
            for g in SSR_GLOBALS {
                let mut pos = 0;
                while let Some(p) = line[pos..].find(g) {
                    let abs = pos + p;
                    let before = if abs == 0 { ' ' } else { line.as_bytes()[abs - 1] as char };
                    let after = line.as_bytes().get(abs + g.len()).map(|b| *b as char).unwrap_or(' ');
                    let is_word = before.is_alphanumeric() || before == '_' || before == '.';
                    let is_word_after = after.is_alphanumeric() || after == '_';
                    if !is_word && !is_word_after {
                        diagnostics.push(Diagnostic {
                            path: ctx.path.to_path_buf(),
                            line: base_line + idx + 1,
                            column: abs + 1,
                            rule_id: super::META.id.into(),
                            message: format!(
                                "`{g}` at the top of `<script setup>` crashes during SSR. Wrap in `onMounted(() => {{ ... }})`."
                            ),
                            severity: Severity::Error,
                            span: None,
                        });
                        break;
                    }
                    pos = abs + g.len();
                }
            }
        }
        for b in line.bytes() {
            match b {
                b'{' | b'(' | b'[' => depth += 1,
                b'}' | b')' | b']' => depth -= 1,
                _ => {}
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
    fn flags_window_top_level() {
        let sfc = "<script setup>\nconst w = window.innerWidth\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn flags_document_top_level() {
        let sfc = "<script setup>\nconst t = document.title\n</script>";
        assert_eq!(run(sfc).len(), 1);
    }

    #[test]
    fn allows_window_in_onmounted() {
        let sfc = "<script setup>\nonMounted(() => {\n  const w = window.innerWidth\n})\n</script>";
        assert!(run(sfc).is_empty());
    }

    #[test]
    fn ignores_plain_script() {
        let sfc = "<script>\nconst w = window.innerWidth\n</script>";
        assert!(run(sfc).is_empty());
    }
}
