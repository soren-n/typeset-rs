//! serialize: EdslDoc → SerialDoc (serialize in order to normalize)
//!
//! Flattens the Edsl arena into the flat serial entry list. The original threaded
//! four accumulator closures (terms, comps, glue, result) plus counters
//! through a CPS recursion on the native stack, which aborts on deep inputs.
//!
//! Here the same computation runs as an explicit left-to-right DFS:
//!
//! - `i`/`j` (group/seq and pack indices) are mutable counters advanced in DFS
//!   pre-order, exactly as the recursion threaded them.
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
//! composition's scope open/close deltas in the same sweep — byte-identical to
//! the recursive version.

use crate::compiler::types::{
    Attr, Break, EdslDoc, EdslId, EdslNode, NO_PATH, PathId, PathNode, Prop, Scope, ScopeRange,
    SerialComp, SerialDoc, SerialEntry, Term, TermLeaf,
};

/// A grp/seq wrapper accumulated on the path to a composition.
#[derive(Copy, Clone)]
enum CompWrap {
    Grp(u64),
    Seq(u64),
}

/// Index into the comp arena; [`NO_COMP`] is the empty (root) accumulator.
type CompId = u32;

/// The empty comp accumulator: no enclosing grp/seq.
const NO_COMP: CompId = u32::MAX;

/// One grp/seq wrapper in the shared comp arena. `parent` links to the
/// next-outer wrapper ([`NO_COMP`] at the outermost) and `depth` is the chain
/// length (root = 0), so two comps' enclosing chains — which share their outer
/// spine by id — can be diffed by an O(delta) longest-common-suffix walk
/// (advance the deeper to equal depth, then step in lockstep to the shared id).
#[derive(Copy, Clone)]
struct CompNode {
    wrap: CompWrap,
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

/// One emitted leaf: its term (text borrowed from the layout arena, `'a`) and
/// how it glues to what follows (an id into the comp arena).
struct Entry<'a> {
    glue: Glue,
    term: Term<'a>,
}

/// A pending subtree to visit, with its scoped path state. `i`/`j` are global
/// counters and deliberately not carried here.
struct Work {
    node: EdslId,
    terms: PathId,
    comps: CompId,
    glue: Glue,
    fixed: bool,
}

/// The output borrows only the layout arena's text (`'a`). Every accumulator is
/// a flat arena owned by this pass, so nothing outlives the return.
pub fn serialize<'a>(doc: &EdslDoc<'a>) -> SerialDoc<'a> {
    let mut i: u64 = 0;
    let mut j: u64 = 0;
    let mut entries: Vec<Entry<'a>> = Vec::new();
    let mut paths: Vec<PathNode> = Vec::new();
    // Shared parent-linked arena of grp/seq wrappers; a `comps` accumulator is
    // an id into it (see [`CompNode`]).
    let mut comp_arena: Vec<CompNode> = Vec::new();

    // Right-to-left visitation is achieved by a stack: pushing the right child
    // before the left makes the left pop (and fully process) first, so the
    // counters thread left-to-right just as the recursion did.
    let mut stack: Vec<Work> = vec![Work {
        node: doc.root,
        terms: NO_PATH,
        comps: NO_COMP,
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
        match &doc.nodes[node as usize] {
            // A leaf: emit its term under the accumulated wrapper path.
            leaf @ (EdslNode::Null | EdslNode::Text(_)) => {
                let leaf = match leaf {
                    EdslNode::Text(data) => TermLeaf::Text(data),
                    _ => TermLeaf::Null,
                };
                entries.push(Entry {
                    glue,
                    term: Term { path: terms, leaf },
                });
            }
            EdslNode::Fix(child) => stack.push(Work {
                node: *child,
                terms,
                comps,
                glue,
                fixed: true,
            }),
            // A grp/seq wrapper: push it onto the comp accumulator (assigning
            // the next scope index in DFS pre-order) and descend.
            wrapper @ (EdslNode::Grp(child) | EdslNode::Seq(child)) => {
                let index = i;
                i += 1;
                let wrap = match wrapper {
                    EdslNode::Grp(_) => CompWrap::Grp(index),
                    _ => CompWrap::Seq(index),
                };
                let parent_depth = if comps == NO_COMP {
                    0
                } else {
                    comp_arena[comps as usize].depth
                };
                let id = comp_arena.len() as CompId;
                comp_arena.push(CompNode {
                    wrap,
                    parent: comps,
                    depth: parent_depth + 1,
                });
                stack.push(Work {
                    node: *child,
                    terms,
                    comps: id,
                    glue,
                    fixed,
                });
            }
            // A nest/pack wrapper: push it onto the path arena (pack assigning
            // the next pack index in DFS pre-order) and descend.
            wrapper @ (EdslNode::Nest(child) | EdslNode::Pack(child)) => {
                let prop = match wrapper {
                    EdslNode::Nest(_) => Prop::Nest,
                    _ => {
                        let index = j;
                        j += 1;
                        Prop::Pack(index)
                    }
                };
                let path = paths.len() as PathId;
                paths.push(PathNode {
                    prop,
                    parent: terms,
                });
                stack.push(Work {
                    node: *child,
                    terms: path,
                    comps,
                    glue,
                    fixed,
                });
            }
            EdslNode::Line(left, right) => {
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
            EdslNode::Comp(left, right, attr) => {
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
    // the previous composition's enclosing scope list *on the same line*; it
    // resets at every line break (Line/Last), because grp/seq scopes never cross
    // a hard line — resolve_scopes resolves each line independently. Diffing the
    // shared-tail `CompList`s is O(delta), so this whole pass stays linear even
    // when scopes nest n deep.
    let mut items: Vec<SerialEntry<'a>> = Vec::with_capacity(entries.len());
    let mut scopes: Vec<Scope> = Vec::new();
    // Scratch for one composition's deltas, reused across entries; each is
    // copied into the shared scope buffer as a range.
    let mut opens: Vec<Scope> = Vec::new();
    let mut closes: Vec<Scope> = Vec::new();
    let mut prev: CompId = NO_COMP;
    for entry in entries.iter() {
        let item = match entry.glue {
            Glue::Last => {
                prev = NO_COMP;
                SerialEntry::Last(entry.term)
            }
            Glue::Line => {
                prev = NO_COMP;
                SerialEntry::Next(entry.term, SerialComp::Line)
            }
            Glue::Comp { comps, attr } => {
                opens.clear();
                closes.clear();
                diff_comps(&comp_arena, prev, comps, &mut opens, &mut closes);
                prev = comps;
                let comp = SerialComp::Comp(
                    attr,
                    append_scopes(&mut scopes, &opens),
                    append_scopes(&mut scopes, &closes),
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

/// Appends `delta` to the shared scope buffer, returning its range.
fn append_scopes(scopes: &mut Vec<Scope>, delta: &[Scope]) -> ScopeRange {
    let start = scopes.len() as u32;
    scopes.extend_from_slice(delta);
    ScopeRange {
        start,
        end: scopes.len() as u32,
    }
}

/// Diffs two enclosing-scope chains (innermost-first, sharing an outer spine by
/// id) into the scopes that *open* (in `cur`, not `prev`) and *close* (in
/// `prev`, not `cur`) at this composition, appended to the caller's scratch.
/// Order within each chain is irrelevant: resolve_scopes keys scopes by index.
/// O(number of scopes that differ).
fn diff_comps(
    arena: &[CompNode],
    prev: CompId,
    cur: CompId,
    opens: &mut Vec<Scope>,
    closes: &mut Vec<Scope>,
) {
    fn scope_of(wrap: CompWrap) -> Scope {
        match wrap {
            CompWrap::Grp(index) => Scope::Grp(index),
            CompWrap::Seq(index) => Scope::Seq(index),
        }
    }
    fn depth(arena: &[CompNode], id: CompId) -> u32 {
        if id == NO_COMP {
            0
        } else {
            arena[id as usize].depth
        }
    }
    let mut a = prev; // contributes closes
    let mut b = cur; // contributes opens
    let (mut da, mut db) = (depth(arena, a), depth(arena, b));
    // Drop the deeper chain's excess head down to the shallower chain's depth.
    while da > db {
        let node = arena[a as usize];
        closes.push(scope_of(node.wrap));
        a = node.parent;
        da -= 1;
    }
    while db > da {
        let node = arena[b as usize];
        opens.push(scope_of(node.wrap));
        b = node.parent;
        db -= 1;
    }
    // Equal depth: step in lockstep until the shared spine — the first id both
    // chains agree on (or both `NO_COMP`) — everything above it differs.
    while a != b {
        let na = arena[a as usize];
        let nb = arena[b as usize];
        closes.push(scope_of(na.wrap));
        opens.push(scope_of(nb.wrap));
        a = na.parent;
        b = nb.parent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::{Pad, push_node};

    /// Deeper than a native-stack recursion could survive (~hundreds of levels
    /// on a 2 MB stack). Reaching it without aborting proves iteration.
    const DEEP: usize = 50_000;

    /// Wraps a `Text` leaf in `DEEP` layers of `wrap`.
    fn deep_unary(text: &'static str, wrap: fn(EdslId) -> EdslNode<'static>) -> EdslDoc<'static> {
        let mut nodes: Vec<EdslNode> = Vec::new();
        let mut cur = push_node(&mut nodes, EdslNode::Text(text));
        for _ in 0..DEEP {
            cur = push_node(&mut nodes, wrap(cur));
        }
        EdslDoc { nodes, root: cur }
    }

    #[test]
    fn serialize_handles_deep_comp_chain() {
        let attr = Attr {
            pad: Pad::Unpadded,
            brk: Break::Breakable,
        };
        // Right-nested Comp chain of DEEP compositions over DEEP + 1 texts.
        let mut nodes: Vec<EdslNode> = Vec::new();
        let mut cur = push_node(&mut nodes, EdslNode::Text("z"));
        for _ in 0..DEEP {
            let left = push_node(&mut nodes, EdslNode::Text("y"));
            cur = push_node(&mut nodes, EdslNode::Comp(left, cur, attr));
        }
        let doc = EdslDoc { nodes, root: cur };
        let serial = serialize(&doc);
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
        let doc = deep_unary("x", EdslNode::Nest);
        let serial = serialize(&doc);
        // Single leaf: one Last whose term sits under a Nest^DEEP path.
        let [SerialEntry::Last(term)] = serial.entries[..] else {
            panic!("expected a single Last")
        };
        let mut count = 0usize;
        let mut cur = term.path;
        while cur != NO_PATH {
            assert!(matches!(serial.paths[cur as usize].prop, Prop::Nest));
            count += 1;
            cur = serial.paths[cur as usize].parent;
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn serialize_handles_deep_pack_chain_indices() {
        let doc = deep_unary("x", EdslNode::Pack);
        let serial = serialize(&doc);
        let [SerialEntry::Last(term)] = serial.entries[..] else {
            panic!("expected a single Last")
        };
        // The outermost Pack is entered first and gets index 0; indices then
        // increase inward. The term's path starts at the innermost wrapper, so
        // walking outward counts back down to 0.
        let mut expected = DEEP as u64;
        let mut cur = term.path;
        while cur != NO_PATH {
            let PathNode {
                prop: Prop::Pack(index),
                parent,
            } = serial.paths[cur as usize]
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
        // Deep grp nesting exercises the i counter and CompList/stack depth.
        let doc = deep_unary("x", EdslNode::Grp);
        // Should not overflow; a single leaf yields a trivial one-Last serial.
        let serial = serialize(&doc);
        assert!(matches!(serial.entries[..], [SerialEntry::Last(_)]));
    }
}
