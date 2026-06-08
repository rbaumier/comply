use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, OxcCheck};
use std::sync::Arc;

pub struct Check;

fn is_nestjs_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@nestjs/")
        || crate::oxc_helpers::source_contains(source, "@Module")
        || crate::oxc_helpers::source_contains(source, "@Injectable")
}

impl OxcCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@nestjs/", "@Module", "@Injectable"])
    }

    fn run_on_semantic<'a>(
        &self,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        ctx: &CheckCtx,
    ) -> Vec<Diagnostic> {
        if !is_nestjs_file(ctx.source) {
            return Vec::new();
        }
        ctx.source
            .lines()
            .enumerate()
            .filter_map(|(idx, line)| {
                let trimmed = line.trim_start();
                if trimmed.starts_with("import ") {
                    return None;
                }
                let col = line.find("forwardRef(")?;
                Some(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: super::META.id.into(),
                    message: "`forwardRef(() => ...)` papers over a circular dependency — \
                              restructure the graph (extract a shared interface, or split the \
                              cycle into a third module)."
                        .into(),
                    severity: Severity::Warning,
                    span: None,
                })
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
    fn flags_forwardref_in_constructor() {
        let src = "import { Inject, Injectable, forwardRef } from '@nestjs/common';\n\
                   @Injectable() class A { constructor(@Inject(forwardRef(() => B)) b: B) {} }";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn flags_forwardref_in_module_imports() {
        let src = "import { Module, forwardRef } from '@nestjs/common';\n\
                   @Module({ imports: [forwardRef(() => OtherModule)] }) class M {}";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn ignores_import_line_only() {
        let src = "import { forwardRef } from '@nestjs/common';\n@Module({}) class M {}";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_non_nestjs_files() {
        let src = "function forwardRef(x) { return x; }\nforwardRef(() => 1);";
        assert!(run(src).is_empty());
    }
}
