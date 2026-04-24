//! rust-no-arc-mutex-tree — `Arc<Mutex<Node>>` / `Rc<RefCell<Node>>`
//! tree or graph structures signal that an arena is the right answer.
//!
//! Shared ownership + interior mutability for every node incurs a
//! heap allocation and an atomic/refcount bump per link, makes cycle
//! detection impossible without extra bookkeeping, and scatters the
//! data across the heap, destroying cache locality. Arena allocators
//! (`id_arena`, `indextree`, `slotmap`, `generational-arena`) use a
//! single contiguous `Vec<T>` plus indices — cheaper, simpler,
//! cycle-friendly.

mod rust;

use crate::diagnostic::Severity;
use crate::rules::meta::RuleMeta;
use crate::rules::RuleDef;

pub const META: RuleMeta = RuleMeta {
    id: "rust-no-arc-mutex-tree",
    description: "Tree/graph node typed as `Arc<Mutex<_>>` or `Rc<RefCell<_>>`.",
    remediation: "Replace the shared-ownership + interior-mutability pattern \
                  with an arena (`id_arena`, `indextree`, `slotmap`, \
                  `generational-arena`). Nodes become indices into a single \
                  `Vec<T>`, which is cheaper, cache-friendly, and makes \
                  cycles trivially representable.",
    severity: Severity::Warning,
    doc_url: None,
    categories: &["rust"],
};

pub fn register() -> RuleDef {
    crate::register_rust_only!(META, rust)
}
