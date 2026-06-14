use crate::diagnostic::Diagnostic;
use crate::oxc_helpers::byte_offset_to_line_col;
use crate::rules::backend::{AstKind, AstType, CheckCtx, OxcCheck};
use oxc_ast::ast::Expression;
use std::sync::Arc;

const TAILWIND_PREFIXES: &[&str] = &[
    "bg-",
    "text-",
    "border-",
    "ring-",
    "shadow-",
    "from-",
    "to-",
    "via-",
    "fill-",
    "stroke-",
    "outline-",
    "accent-",
    "caret-",
    "divide-",
    "placeholder-",
];

pub struct Check;

impl OxcCheck for Check {
    fn interested_kinds(&self) -> &'static [AstType] {
        &[AstType::TemplateLiteral]
    }

    fn run<'a>(
        &self,
        node: &oxc_semantic::AstNode<'a>,
        ctx: &CheckCtx,
        _semantic: &'a oxc_semantic::Semantic<'a>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Tailwind prefixes like `bg-`/`border-` collide with BEM and custom
        // design-system class names. Without Tailwind, a template literal that
        // builds a CSS class string is not a purge hazard.
        if !ctx.project.uses_tailwind() {
            return;
        }
        let AstKind::TemplateLiteral(tpl) = node.kind() else {
            return;
        };
        // Must have at least one expression (substitution).
        if tpl.expressions.is_empty() {
            return;
        }
        // A CSS value expression (`calc(${v} * var(--tw-...))`) is not a class
        // name: it builds a declaration value, never a purgeable class string.
        // `calc(`/`var(` never appear in a Tailwind utility class reference.
        let is_css_value = tpl
            .quasis
            .iter()
            .any(|q| q.value.raw.contains("calc(") || q.value.raw.contains("var("));
        if is_css_value {
            return;
        }
        // Reconstruct the template text from quasis to check for Tailwind prefixes.
        let has_tw_prefix = tpl.quasis.iter().any(|q| {
            let raw = q.value.raw.as_str();
            TAILWIND_PREFIXES.iter().any(|p| raw.contains(p))
        });
        if !has_tw_prefix {
            return;
        }
        // A template literal passed to a utility-registration API
        // (`designSystem.utilities.static`/`.suggest`/`.functional`) defines a
        // utility *name*, not a class consumed in markup, so purge visibility
        // does not apply.
        if is_utility_registration_arg(node, _semantic) {
            return;
        }
        let (line, column) = byte_offset_to_line_col(ctx.source, tpl.span.start as usize);
        diagnostics.push(Diagnostic {
            path: Arc::clone(&ctx.path_arc),
            line,
            column,
            rule_id: super::META.id.into(),
            message: "Dynamic Tailwind class via template literal — \
                      purge only sees full static strings, so the \
                      generated class won't ship. Use a static map: \
                      `const colors = { blue: 'bg-blue-500', ... }`."
                .into(),
            severity: super::META.severity,
            span: None,
        });
    }
}

/// Utility-definition methods on a Tailwind `utilities` registry. Passing a
/// template literal here builds the utility's *name*, not a class reference.
const REGISTRATION_METHODS: &[&str] = &["static", "suggest", "functional"];

/// True when `node` is an argument to a `<...>.utilities.<method>(...)` call
/// whose `<method>` registers a utility (`static`/`suggest`/`functional`).
fn is_utility_registration_arg<'a>(
    node: &oxc_semantic::AstNode<'a>,
    semantic: &'a oxc_semantic::Semantic<'a>,
) -> bool {
    let nodes = semantic.nodes();
    // Walk up to the enclosing call: a template-literal argument sits directly
    // under the `CallExpression`, or under an `Argument`/parenthesis wrapper.
    let mut current = node.id();
    for _ in 0..3 {
        let parent = nodes.parent_id(current);
        if parent == current {
            return false;
        }
        if let AstKind::CallExpression(call) = nodes.kind(parent) {
            let Expression::StaticMemberExpression(member) = &call.callee else {
                return false;
            };
            if !REGISTRATION_METHODS.contains(&member.property.name.as_str()) {
                return false;
            }
            // The receiver chain must reference a `utilities` registry.
            return matches!(
                &member.object,
                Expression::StaticMemberExpression(obj)
                    if obj.property.name.as_str() == "utilities"
            );
        }
        current = parent;
    }
    false
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

    fn run_on(source: &str) -> Vec<Diagnostic> {
        let project = crate::project::ProjectCtx::empty_with_tailwind();
        let file = crate::rules::file_ctx::default_static_file_ctx();
        crate::rules::test_helpers::run_rule_with_ctx(&Check, source, "t.ts", &project, file)
    }

    fn run_without_tailwind(source: &str) -> Vec<Diagnostic> {
        crate::rules::test_helpers::run_rule(&Check, source, "t.ts")
    }

    #[test]
    fn flags_bg_dynamic() {
        assert_eq!(run_on("const c = `bg-${color}-500`;").len(), 1);
    }

    #[test]
    fn flags_text_dynamic() {
        assert_eq!(run_on("const c = `text-${size}-xl`;").len(), 1);
    }

    #[test]
    fn allows_static_class() {
        assert!(run_on("const c = 'bg-blue-500';").is_empty());
    }

    #[test]
    fn allows_non_tailwind_template_literal() {
        assert!(run_on("const c = `hello ${name}`;").is_empty());
    }

    // https://github.com/rbaumier/comply/issues/1613 — Angular CDK builds its
    // own BEM-style CSS class names with `border-` segments; the project has no
    // Tailwind, so this must stay silent.
    #[test]
    fn silent_when_project_has_no_tailwind() {
        assert!(run_without_tailwind("const c = `${prefix}-border-elem-top`;").is_empty());
    }

    // https://github.com/rbaumier/comply/issues/2158 — the tailwindcss source
    // registers its own utility names through `designSystem.utilities.static`/
    // `.suggest`; the template literal there defines a utility name, not a class
    // consumed in markup, so purge visibility does not apply.
    #[test]
    fn allows_utility_registration_static() {
        assert!(
            run_on("designSystem.utilities.static(`bg-gradient-to-${value}`, () => []);").is_empty()
        );
    }

    #[test]
    fn allows_utility_registration_suggest() {
        assert!(run_on("designSystem.utilities.suggest(`bg-gradient-to-${value}`, () => []);").is_empty());
    }

    // A CSS value built from `calc(...)`/`var(...)` is a declaration value, not
    // a class name.
    #[test]
    fn allows_css_value_template() {
        assert!(run_on(
            "decl('border-inline-start-width', `calc(${value} * var(--tw-divide-x-reverse))`);"
        )
        .is_empty());
    }

    // Negative space: a genuine dynamic class in markup is still flagged, even
    // when wrapped in a non-registration call like `clsx`.
    #[test]
    fn still_flags_dynamic_class_in_clsx() {
        assert_eq!(run_on("clsx(`bg-${color}-500`);").len(), 1);
    }

    // A `.static`/`.suggest` call on a receiver that is not a `utilities`
    // registry is not a Tailwind utility definition and stays flagged.
    #[test]
    fn still_flags_non_utilities_static_call() {
        assert_eq!(run_on("registry.static(`bg-${color}-500`, () => []);").len(), 1);
    }
}
