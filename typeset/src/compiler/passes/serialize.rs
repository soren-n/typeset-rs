//! serialize: LayoutArena → SerialDoc (serialize in order to normalize)
//!
//! Flattens the (break-resolved) layout arena into the flat serial entry list
//! with an explicit left-to-right DFS:
//!
//! - `i`/`j` (group/seq and pack indices) are counters advanced in DFS
//!   pre-order.
//! - The nest/pack path accumulator `terms` is an id into the output's shared
//!   path arena — descending through a wrapper pushes one arena node, so
//!   sibling leaves share their path spine and a term is just (path id, leaf).
//! - The scope accumulator `comps` (grp/seq) is an id into a shared
//!   parent-linked comp arena (one node per wrapper descended through, exactly
//!   like the path arena) — pushing a wrapper is O(1) and snapshots (captured
//!   by a composition's glue) share their outer spine by id.
//! - `glue` (how a leaf's term attaches to what follows: Last, Line, or a
//!   Comp separator) is carried per work item.
//!
//! Each leaf emits one `(glue, term)` entry; a final forward pass resolves the
//! entries (in leaf order) into the `SerialEntry` list, computing each
//! composition's scope open/close deltas in the same sweep.

use super::flatten::{LayId, LayoutArena, LayoutNode};
use crate::compiler::types::{
    Arena, Attr, Break, Id, PathId, PathNode, Prop, Range, Scope, ScopeKind, Term, TermLeaf,
    append_range,
};

/// A flat list of leaf entries in document order; each entry is a term plus
/// how it glues to what follows. The entry list is always non-empty and its
/// final entry is always `Last`. The document owns the path arena the
/// entries' terms point into and the scope buffer their deltas range into,
/// and borrows only the layout text buffer.
#[derive(Debug)]
pub(crate) struct SerialDoc<'a> {
    pub(crate) entries: Vec<SerialEntry<'a>>,
    /// The shared nest/pack path arena every [`Term`]'s `path` points into.
    pub(crate) paths: Arena<PathNode>,
    /// The shared scope buffer every delta ranges into.
    pub(crate) scopes: Vec<Scope>,
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum SerialEntry<'a> {
    /// A term followed by a composition — a hard line break
    /// (`SerialComp::Line`) or a composition separator (`SerialComp::Comp`).
    Next(Term<'a>, SerialComp),
    /// The document's final term, with nothing following.
    Last(Term<'a>),
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum SerialComp {
    Line,
    /// A composition: its attributes, the scopes opening here, and the scopes
    /// closing here (ranges into the document's shared scope buffer).
    Comp(Attr, Range<Scope>, Range<Scope>),
}

/// A comp accumulator: the innermost enclosing grp/seq wrapper, `None` at the
/// root.
type CompId = Option<Id<CompNode>>;

/// One grp/seq wrapper in the shared comp arena. `parent` links to the
/// next-outer wrapper (`None` at the outermost) and `depth` is the chain
/// length (root = 0), so two comps' enclosing chains — which share their outer
/// spine by id — can be diffed by an O(delta) longest-common-suffix walk
/// (advance the deeper to equal depth, then step in lockstep to the shared id).
#[derive(Copy, Clone)]
struct CompNode {
    scope: Scope,
    parent: CompId,
    depth: u32,
}

/// How a leaf's term attaches to the rest of the serial.
#[derive(Copy, Clone)]
enum Glue {
    /// The final term of the whole document.
    Last,
    /// A hard line break follows.
    Line,
    /// A composition follows, wrapped by the captured comp accumulator.
    Comp { comps: CompId, attr: Attr },
}

/// One emitted leaf: its term (text borrowed from the layout buffer, `'a`) and
/// how it glues to what follows.
struct Entry<'a> {
    glue: Glue,
    term: Term<'a>,
}

/// A pending subtree to visit, with its scoped path state. `i`/`j` are global
/// counters and deliberately not carried here.
struct Work {
    node: LayId,
    terms: Option<PathId>,
    comps: CompId,
    glue: Glue,
    fixed: bool,
}

/// The output borrows only `text` (the layout text buffer). Every accumulator
/// is a flat arena owned by this pass, so nothing else outlives the return.
pub fn serialize<'a>(doc: &LayoutArena, text: &'a str) -> SerialDoc<'a> {
    let mut i: u32 = 0;
    let mut j: u32 = 0;
    let mut entries: Vec<Entry<'a>> = Vec::new();
    let mut paths: Arena<PathNode> = Arena::new();
    // Shared parent-linked arena of grp/seq wrappers; a `comps` accumulator is
    // an id into it (see [`CompNode`]).
    let mut comp_arena: Arena<CompNode> = Arena::new();

    // Right-to-left visitation is achieved by a stack: pushing the right child
    // before the left makes the left pop (and fully process) first, so the
    // counters thread left-to-right.
    let mut stack: Vec<Work> = vec![Work {
        node: doc.root,
        terms: None,
        comps: None,
        glue: Glue::Last,
        fixed: false,
    }];

    while let Some(work) = stack.pop() {
        let Work {
            node,
            terms,
            comps,
            glue,
            fixed,
        } = work;
        match &doc.nodes[node] {
            // A leaf: emit its term under the accumulated wrapper path.
            leaf @ (LayoutNode::Null | LayoutNode::Text(_)) => {
                let leaf = match leaf {
                    LayoutNode::Text(range) => TermLeaf::Text(range.slice(text)),
                    _ => TermLeaf::Null,
                };
                entries.push(Entry {
                    glue,
                    term: Term { path: terms, leaf },
                });
            }
            LayoutNode::Fix(child) => stack.push(Work {
                node: *child,
                terms,
                comps,
                glue,
                fixed: true,
            }),
            // A grp/seq wrapper: push it onto the comp accumulator (assigning
            // the next scope index in DFS pre-order) and descend.
            wrapper @ (LayoutNode::Grp(child) | LayoutNode::Seq(child)) => {
                let index = i;
                i += 1;
                let kind = match wrapper {
                    LayoutNode::Grp(_) => ScopeKind::Grp,
                    _ => ScopeKind::Seq,
                };
                let depth = comps.map_or(0, |id| comp_arena[id].depth) + 1;
                let id = comp_arena.push(CompNode {
                    scope: Scope { kind, index },
                    parent: comps,
                    depth,
                });
                stack.push(Work {
                    node: *child,
                    terms,
                    comps: Some(id),
                    glue,
                    fixed,
                });
            }
            // A nest/pack wrapper: push it onto the path arena (pack assigning
            // the next pack index in DFS pre-order) and descend.
            wrapper @ (LayoutNode::Nest(child) | LayoutNode::Pack(child)) => {
                let prop = match wrapper {
                    LayoutNode::Nest(_) => Prop::Nest,
                    _ => {
                        let index = j;
                        j += 1;
                        Prop::Pack(index)
                    }
                };
                let path = paths.push(PathNode {
                    prop,
                    parent: terms,
                });
                stack.push(Work {
                    node: *child,
                    terms: Some(path),
                    comps,
                    glue,
                    fixed,
                });
            }
            LayoutNode::Line(left, right) => {
                // Right inherits the outer glue; left's trailing term gets a
                // hard line. Push right first so left is processed first.
                stack.push(Work {
                    node: *right,
                    terms,
                    comps,
                    glue,
                    fixed,
                });
                stack.push(Work {
                    node: *left,
                    terms,
                    comps,
                    glue: Glue::Line,
                    fixed,
                });
            }
            LayoutNode::Comp(left, right, attr) => {
                // Inside a fix wrapper every composition is fixed.
                let attr1 = Attr {
                    pad: attr.pad,
                    brk: if fixed { Break::Fixed } else { attr.brk },
                };
                let comp_glue = Glue::Comp { comps, attr: attr1 };
                stack.push(Work {
                    node: *right,
                    terms,
                    comps,
                    glue,
                    fixed,
                });
                stack.push(Work {
                    node: *left,
                    terms,
                    comps,
                    glue: comp_glue,
                    fixed,
                });
            }
        }
    }

    // Resolve each leaf entry into a `SerialEntry`, computing every
    // composition's scope open/close deltas in the same forward pass. `prev` is
    // the previous composition's enclosing scope chain *on the same line*; it
    // resets at every line break (Line/Last), because grp/seq scopes never cross
    // a hard line — resolve_scopes resolves each line independently. Diffing the
    // shared-spine `CompNode` chains is O(delta), so this whole pass stays linear
    // even when scopes nest n deep.
    let mut items: Vec<SerialEntry<'a>> = Vec::with_capacity(entries.len());
    let mut scopes: Vec<Scope> = Vec::new();
    // Scratch for one composition's deltas, reused across entries; each is
    // copied into the shared scope buffer as a range.
    let mut opens: Vec<Scope> = Vec::new();
    let mut closes: Vec<Scope> = Vec::new();
    let mut prev: CompId = None;
    for entry in entries.iter() {
        let item = match entry.glue {
            Glue::Last => {
                prev = None;
                SerialEntry::Last(entry.term)
            }
            Glue::Line => {
                prev = None;
                SerialEntry::Next(entry.term, SerialComp::Line)
            }
            Glue::Comp { comps, attr } => {
                opens.clear();
                closes.clear();
                diff_comps(&comp_arena, prev, comps, &mut opens, &mut closes);
                prev = comps;
                let comp = SerialComp::Comp(
                    attr,
                    append_range(&mut scopes, &opens),
                    append_range(&mut scopes, &closes),
                );
                SerialEntry::Next(entry.term, comp)
            }
        };
        items.push(item);
    }
    SerialDoc {
        entries: items,
        paths,
        scopes,
    }
}

/// Diffs two enclosing-scope chains (innermost-first, sharing an outer spine by
/// id) into the scopes that *open* (in `cur`, not `prev`) and *close* (in
/// `prev`, not `cur`) at this composition, appended to the caller's scratch.
/// Order within each chain is irrelevant: resolve_scopes keys scopes by index.
/// O(number of scopes that differ).
fn diff_comps(
    arena: &Arena<CompNode>,
    prev: CompId,
    cur: CompId,
    opens: &mut Vec<Scope>,
    closes: &mut Vec<Scope>,
) {
    let depth = |id: CompId| id.map_or(0, |id| arena[id].depth);
    let mut a = prev; // contributes closes
    let mut b = cur; // contributes opens
    let (mut da, mut db) = (depth(a), depth(b));
    // Drop the deeper chain's excess head down to the shallower chain's depth.
    while da > db {
        let node = arena[a.expect("deeper chain is non-empty")];
        closes.push(node.scope);
        a = node.parent;
        da -= 1;
    }
    while db > da {
        let node = arena[b.expect("deeper chain is non-empty")];
        opens.push(node.scope);
        b = node.parent;
        db -= 1;
    }
    // Equal depth: step in lockstep until the shared spine — the first id both
    // chains agree on (or both `None`) — everything above it differs.
    while a != b {
        let na = arena[a.expect("chains of equal depth")];
        let nb = arena[b.expect("chains of equal depth")];
        closes.push(na.scope);
        opens.push(nb.scope);
        a = na.parent;
        b = nb.parent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::Pad;

    /// Deeper than a native-stack recursion could survive (~hundreds of levels
    /// on a 2 MB stack). Reaching it without aborting proves iteration.
    const DEEP: usize = 50_000;

    /// A one-character text buffer and the range covering it.
    const TEXT: &str = "x";
    const X: Range<str> = Range::new(0, 1);

    /// Wraps a `Text` leaf in `DEEP` layers of `wrap`.
    fn deep_unary(wrap: fn(LayId) -> LayoutNode) -> LayoutArena {
        let mut nodes: Arena<LayoutNode> = Arena::new();
        let mut cur = nodes.push(LayoutNode::Text(X));
        for _ in 0..DEEP {
            cur = nodes.push(wrap(cur));
        }
        LayoutArena { nodes, root: cur }
    }

    #[test]
    fn serialize_handles_deep_comp_chain() {
        let attr = Attr {
            pad: Pad::Unpadded,
            brk: Break::Breakable,
        };
        // Right-nested Comp chain of DEEP compositions over DEEP + 1 texts.
        let mut nodes: Arena<LayoutNode> = Arena::new();
        let mut cur = nodes.push(LayoutNode::Text(X));
        for _ in 0..DEEP {
            let left = nodes.push(LayoutNode::Text(X));
            cur = nodes.push(LayoutNode::Comp(left, cur, attr));
        }
        let doc = LayoutArena { nodes, root: cur };
        let serial = serialize(&doc, TEXT);
        // DEEP Next entries, then a final Last entry.
        let count = serial
            .entries
            .iter()
            .filter(|e| matches!(e, SerialEntry::Next(..)))
            .count();
        assert_eq!(count, DEEP);
        assert!(matches!(serial.entries.last(), Some(SerialEntry::Last(_))));
    }

    #[test]
    fn serialize_handles_deep_nest_chain() {
        let doc = deep_unary(LayoutNode::Nest);
        let serial = serialize(&doc, TEXT);
        // Single leaf: one Last whose term sits under a Nest^DEEP path.
        let [SerialEntry::Last(term)] = serial.entries[..] else {
            panic!("expected a single Last")
        };
        let mut count = 0usize;
        let mut cur = term.path;
        while let Some(id) = cur {
            assert!(matches!(serial.paths[id].prop, Prop::Nest));
            count += 1;
            cur = serial.paths[id].parent;
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn serialize_handles_deep_pack_chain_indices() {
        let doc = deep_unary(LayoutNode::Pack);
        let serial = serialize(&doc, TEXT);
        let [SerialEntry::Last(term)] = serial.entries[..] else {
            panic!("expected a single Last")
        };
        // The outermost Pack is entered first and gets index 0; indices then
        // increase inward. The term's path starts at the innermost wrapper, so
        // walking outward counts back down to 0.
        let mut expected = DEEP as u32;
        let mut cur = term.path;
        while let Some(id) = cur {
            let PathNode {
                prop: Prop::Pack(index),
                parent,
            } = serial.paths[id]
            else {
                panic!("expected a pack wrapper")
            };
            expected -= 1;
            assert_eq!(index, expected);
            cur = parent;
        }
        assert_eq!(expected, 0);
    }

    #[test]
    fn serialize_handles_deep_grp_chain() {
        // Deep grp nesting exercises the i counter and comp-arena/stack depth.
        let doc = deep_unary(LayoutNode::Grp);
        // Should not overflow; a single leaf yields a trivial one-Last serial.
        let serial = serialize(&doc, TEXT);
        assert!(matches!(serial.entries[..], [SerialEntry::Last(_)]));
    }
}
