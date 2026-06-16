//! use-vue-define-macros-order — order Vue compiler macros in `<script setup>`.
//!
//! ## Rationale
//!
//! Ported from Biome's `useVueDefineMacrosOrder`. Vue's Composition-API
//! compiler macros (`defineModel`, `defineProps`, `defineEmits`, …) are
//! auto-imported globals that exist only inside a `<script setup>` block.
//! Keeping them in a consistent order — and before any non-macro statements —
//! makes the component's contract easy to scan.
//!
//! ## What fires
//!
//! Inside a `<script setup>` block, a configured macro call (bare
//! `defineProps({})` or assigned `const props = defineProps()`, including the
//! `withDefaults(defineProps(...))` wrapper) that appears either:
//!
//! - after a macro that should come later in the order, or
//! - after a non-macro statement that is not skippable (imports, type / module
//!   declarations, `debugger`, empty statements and `export` clauses are
//!   skippable).
//!
//! The diagnostic points at the lowest-order macro found out of place.
//!
//! ## What's clean
//!
//! - macros in the configured order, placed before non-macro code;
//! - skippable statements (imports / type declarations / `debugger`) before the
//!   macros;
//! - `withDefaults(defineNotProps(...))` — only `defineProps` is unwrapped from
//!   `withDefaults`.
//!
//! ## Options
//!
//! `order` (`src/config/defaults.toml`) — the macro names in their required
//! order. Default: `["defineModel", "defineProps", "defineEmits"]`. Names not
//! in the list are treated as non-macro statements.
//!
//! ## Language coverage
//!
//! Vue SFC `<script setup>` blocks (extracted with tree-sitter-vue, re-parsed
//! with oxc). The macros do not exist outside `<script setup>`, so the
//! TypeScript / JavaScript / TSX backends are no-ops kept for wiring parity.

mod oxc_typescript;
mod oxc_vue;

use crate::diagnostic::Severity;
use crate::files::Language;
use crate::rules::RuleDef;
use crate::rules::backend::Backend;
use crate::rules::meta::RuleMeta;

pub const META: RuleMeta = RuleMeta {
    id: "use-vue-define-macros-order",
    description: "A Vue compiler macro in `<script setup>` is out of order.",
    remediation: "Order the compiler macros as configured (default \
                  `defineModel`, `defineProps`, `defineEmits`) and place them \
                  before any non-macro statements.",
    severity: Severity::Warning,
    doc_url: Some("https://biomejs.dev/linter/rules/use-vue-define-macros-order/"),
    categories: &["vue"],

    skip_in_test_dir: false,
    skip_in_relaxed_dir: false,
};

pub fn register() -> RuleDef {
    RuleDef {
        meta: META,
        backends: vec![
            (
                Language::TypeScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (
                Language::JavaScript,
                Backend::Oxc(Box::new(oxc_typescript::Check)),
            ),
            (Language::Tsx, Backend::Oxc(Box::new(oxc_typescript::Check))),
            (Language::Vue, Backend::TreeSitter(Box::new(oxc_vue::Check))),
        ],
    }
}
