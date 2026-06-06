//! Flag every call site of `forwardRef(() => ...)` in NestJS code. The
//! presence of `forwardRef` is the marker — restructure the graph instead.

use crate::diagnostic::{Diagnostic, Severity};
use crate::rules::backend::{CheckCtx, TextCheck};
use std::sync::Arc;

#[derive(Debug)]
pub struct Check;

fn is_nestjs_file(source: &str) -> bool {
    crate::oxc_helpers::source_contains(source, "@nestjs/") || crate::oxc_helpers::source_contains(source, "@Module") || crate::oxc_helpers::source_contains(source, "@Injectable")
}

impl TextCheck for Check {
    fn prefilter(&self) -> Option<&'static [&'static str]> {
        Some(&["@nestjs/", "@Module", "@Injectable"])
    }

    fn check(&self, ctx: &CheckCtx) -> Vec<Diagnostic> {
        if !is_nestjs_file(ctx.source) {
            return Vec::new();
        }
        let mut diagnostics = Vec::new();
        for (idx, line) in ctx.source.lines().enumerate() {
            // Skip the import line itself — we flag use sites, not the import.
            let trimmed = line.trim_start();
            if trimmed.starts_with("import ") {
                continue;
            }
            if let Some(col) = line.find("forwardRef(") {
                diagnostics.push(Diagnostic {
                    path: Arc::clone(&ctx.path_arc),
                    line: idx + 1,
                    column: col + 1,
                    rule_id: super::META.id.into(),
                    message: "`forwardRef(() => ...)` papers over a circular dependency — \
                              restructure the graph (extract a shared interface, or split the \
                              cycle into a third module)."
                        .to_string(),
                    severity: Severity::Warning,
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

    fn run(source: &str) -> Vec<Diagnostic> {
        Check.check(&CheckCtx::for_test(Path::new("svc.ts"), source))
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
