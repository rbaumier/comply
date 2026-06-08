use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn find_offenses(source: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(rel) = source[from..].find("setQueryData(") {
        let abs = from + rel;
        let pre = source.as_bytes().get(abs.saturating_sub(1)).copied();
        let boundary = pre.is_none_or(|c| !c.is_ascii_alphanumeric() && c != b'_' && c != b'$');
        if boundary {
            let after = abs + "setQueryData(".len();
            let bs = source.as_bytes();
            let mut i = after;
            while i < bs.len() && bs[i].is_ascii_whitespace() {
                i += 1;
            }
            if bs.get(i) == Some(&b'[') {
                out.push(abs);
            }
        }
        from = abs + 1;
    }
    out
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["setQueryData"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if ctx.file.path_segments.in_test_dir {
            return Vec::new();
        }
        if !ctx.source_contains("setQueryData(") {
            return Vec::new();
        }
        find_offenses(ctx.source)
            .into_iter()
            .map(|offset| {
                let (line, column) = byte_offset_to_line_col(ctx.source, offset);
                Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line,
                    column,
                    rule_id: super::META.id.into(),
                    message: "`setQueryData` uses an inline array key — \
                              extract a query key factory so cache writes are findable."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_oxc_ts(source, &Check)
    }

    #[test]
    fn flags_inline_array_key() {
        let src = "queryClient.setQueryData(['users', id], data);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_inline_array_with_whitespace() {
        let src = "queryClient.setQueryData( ['users', id], data);";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn allows_factory_key() {
        let src = "queryClient.setQueryData(userKeys.detail(id), data);";
        assert!(run(src).is_empty());
    }

    #[test]
    fn allows_variable_key() {
        let src = "queryClient.setQueryData(myKey, data);";
        assert!(run(src).is_empty());
    }
}
