//! zod-brand-ids oxc backend.

use crate::diagnostic::{Diagnostic, Severity};
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_span::GetSpan;
use std::sync::Arc;

/// Return `true` if `key` is an ID-like field name.
fn is_id_like(key: &str) -> bool {
    let key = key.trim_matches(|c: char| c == '"' || c == '\'');
    if key.eq_ignore_ascii_case("id") {
        return true;
    }
    if key.strip_suffix("_id").is_some_and(|p| !p.is_empty()) {
        return true;
    }
    if key.strip_suffix("_ID").is_some_and(|p| !p.is_empty()) {
        return true;
    }
    for suffix in ["Id", "ID"] {
        if let Some(prefix) = key.strip_suffix(suffix) {
            if prefix.is_empty() {
                continue;
            }
            let last = prefix.chars().next_back().unwrap_or(' ');
            if last.is_ascii_lowercase() || last.is_ascii_digit() {
                return true;
            }
        }
    }
    false
}

/// Return `true` if the Zod schema expression's base type is `z.string()`.
///
/// Branding gives a nominal type so distinct IDs are not interchangeable.
/// That only matters for string IDs: TypeScript already keeps `number` and
/// `string` distinct, so numeric foreign keys (`z.number()`, `z.bigint()`,
/// `z.int()`) gain nothing from a brand. We gate on the call-chain root
/// (`z.string()...`) rather than the field name.
fn is_zod_string_base(value_text: &str) -> bool {
    // Skip the leading `z.` then the optional whitespace between member calls
    // (`z.string()\n.min(1)`), and confirm the first method is `string`.
    let after_z = match value_text.strip_prefix("z.") {
        Some(rest) => rest.trim_start(),
        None => return false,
    };
    let method = after_z.trim_start_matches(|c: char| c.is_ascii_alphanumeric() || c == '_');
    let name_len = after_z.len() - method.len();
    &after_z[..name_len] == "string"
}

/// Form-library signals. A Zod schema in a file that wires up React Hook Form
/// (or a `zodResolver`) is a UI form-validation schema, not an API/entity
/// boundary schema: branding a form field is semantically wrong and breaks the
/// form library's type inference. These are framework/usage specifiers, not
/// field-name or value allowlists.
const FORM_RESOLVER_INDICATORS: &[&str] =
    &["react-hook-form", "@hookform/", "zodResolver", "useFormContext"];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::ObjectProperty]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let AstKind::ObjectProperty(prop) = node.kind() else {
            return;
        };

        let key_text = match &prop.key {
            oxc_ast::ast::PropertyKey::StaticIdentifier(id) => id.name.as_str(),
            oxc_ast::ast::PropertyKey::StringLiteral(lit) => lit.value.as_str(),
            _ => return,
        };
        if !is_id_like(key_text) {
            return;
        }

        let value_span = prop.value.span();
        let value_text = &ctx.source[value_span.start as usize..value_span.end as usize];
        if !is_zod_string_base(value_text) {
            return;
        }
        if value_text.contains(".brand(") || value_text.contains(".brand<") {
            return;
        }

        // UI form-validation schemas (React Hook Form / zodResolver) are not
        // entity boundary schemas — don't suggest branding their fields.
        if FORM_RESOLVER_INDICATORS.iter().any(|m| ctx.source_contains(m)) {
            return;
        }

        let (line, column) = byte_offset_to_line_col(ctx.source, prop.key.span().start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: format!(
                "`{}` looks like an ID — add `.brand<\"...\">()` so distinct IDs \
                 are not assignable to each other.",
                key_text.trim_matches(|c: char| c == '"' || c == '\''),
            ),
            severity: Severity::Warning,
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

    fn run(s: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, s, "schema.ts")
    }

    #[test]
    fn flags_string_id_in_backend_schema() {
        assert_eq!(run("z.object({ userId: z.string() })").len(), 1);
        assert_eq!(run("z.object({ customer_id: z.string().uuid() })").len(), 1);
    }

    #[test]
    fn skips_numeric_id_fields() {
        // Case 1: numeric foreign keys are already type-safe without a brand.
        assert!(run("z.object({ brand_id: z.number() })").is_empty());
        assert!(run("z.object({ user_id: z.bigint() })").is_empty());
        assert!(run("z.object({ post_id: z.int() })").is_empty());
    }

    #[test]
    fn skips_non_string_id_fields() {
        assert!(run("z.object({ is_id: z.boolean() })").is_empty());
        assert!(run("z.object({ created_id: z.date() })").is_empty());
    }

    #[test]
    fn skips_form_schema_with_react_hook_form() {
        // Case 2: a Zod schema in a React Hook Form file is a UI form schema.
        let src = "import { useForm } from 'react-hook-form'\n\
                   const schema = z.object({ region_id: z.string().min(1) })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn skips_form_schema_with_zod_resolver() {
        let src = "import { zodResolver } from '@hookform/resolvers/zod'\n\
                   const schema = z.object({ customer_id: z.string().optional() })\n\
                   useForm({ resolver: zodResolver(schema) })";
        assert!(run(src).is_empty());
    }

    #[test]
    fn still_flags_string_id_outside_form_context() {
        // No form-library signal: backend API/entity schema still gets the nudge.
        let src = "const userSchema = z.object({ user_id: z.string().min(1) })";
        assert_eq!(run(src).len(), 1);
    }

    #[test]
    fn skips_already_branded() {
        assert!(run("z.object({ userId: z.string().brand<\"UserId\">() })").is_empty());
    }
}
