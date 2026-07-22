pub mod doc;
pub mod intermediate;
pub mod layout;
pub mod traversal;

pub use doc::*;
pub use intermediate::*;
pub use layout::*;

/// Append a node to a postorder arena and return its index.
///
/// Every intermediate representation stores its nodes in flat `Vec` arenas
/// indexed by `u32` ids; this is the one appender they all share.
pub(crate) fn push_node<T>(arena: &mut Vec<T>, node: T) -> u32 {
    let id = arena.len() as u32;
    arena.push(node);
    id
}
