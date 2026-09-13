//! resolve_breaks: LayoutArena → LayoutArena (collapse broken sequences)
//!
//! Resolves hard line breaks: a composition inside a broken sequence becomes a
//! `Line`, and a seq wrapper whose subtree already breaks is dropped (its
//! content is unconditionally broken anyway).
//!
//! The input is a flat postorder arena, so the pass is three plain loops:
//! 1. mark — forward (bottom-up): does each subtree contain a hard line break?
//! 2. spread — backward (top-down): is each node inside a broken sequence?
//! 3. build — forward: emit the new arena, rewriting compositions to lines
//!    and dropping broken seq wrappers as the flags dictate.

use super::flatten::{LayId, LayoutArena, LayoutNode};
use crate::compiler::types::{Arena, Break, IdVec};

pub fn resolve_breaks(arena: &LayoutArena) -> LayoutArena {
    let n = arena.nodes.len();

    // 1. mark: whether each subtree contains a hard line break. Wrappers pass
    // the flag through; a Line is one; a Comp has one if either operand does.
    let mut has_line: IdVec<LayoutNode, bool> = IdVec::with_capacity(n);
    for (_, node) in arena.nodes.iter() {
        let flag = match *node {
            LayoutNode::Null | LayoutNode::Text(_) => false,
            LayoutNode::Fix(c)
            | LayoutNode::Grp(c)
            | LayoutNode::Seq(c)
            | LayoutNode::Nest(c)
            | LayoutNode::Pack(c) => has_line[c],
            LayoutNode::Line(..) => true,
            LayoutNode::Comp(l, r, _) => has_line[l] || has_line[r],
        };
        has_line.push(flag);
    }

    // 2. spread: whether each node sits inside a broken sequence. Fix and grp
    // reset the context; a seq sets it to its own subtree's flag; everything
    // else passes it down. The root starts outside any sequence.
    let mut brk: IdVec<LayoutNode, bool> = IdVec::filled(false, n);
    for (i, node) in arena.nodes.iter().rev() {
        match *node {
            LayoutNode::Null | LayoutNode::Text(_) => {}
            LayoutNode::Fix(c) | LayoutNode::Grp(c) => brk[c] = false,
            LayoutNode::Seq(c) => brk[c] = has_line[c],
            LayoutNode::Nest(c) | LayoutNode::Pack(c) => brk[c] = brk[i],
            LayoutNode::Line(l, r) | LayoutNode::Comp(l, r, _) => {
                brk[l] = brk[i];
                brk[r] = brk[i];
            }
        }
    }

    // 3. build: emit the resolved arena bottom-up.
    let mut nodes: Arena<LayoutNode> = Arena::with_capacity(n);
    let mut out: IdVec<LayoutNode, LayId> = IdVec::with_capacity(n);
    for (i, node) in arena.nodes.iter() {
        let id = match *node {
            LayoutNode::Null => nodes.push(LayoutNode::Null),
            LayoutNode::Text(range) => nodes.push(LayoutNode::Text(range)),
            LayoutNode::Fix(c) => nodes.push(LayoutNode::Fix(out[c])),
            LayoutNode::Grp(c) => nodes.push(LayoutNode::Grp(out[c])),
            LayoutNode::Seq(c) => {
                // A sequence that already breaks is dropped: its content is
                // unconditionally broken (the flag spread takes care of that).
                if has_line[c] {
                    out[c]
                } else {
                    nodes.push(LayoutNode::Seq(out[c]))
                }
            }
            LayoutNode::Nest(c) => nodes.push(LayoutNode::Nest(out[c])),
            LayoutNode::Pack(c) => nodes.push(LayoutNode::Pack(out[c])),
            LayoutNode::Line(l, r) => nodes.push(LayoutNode::Line(out[l], out[r])),
            LayoutNode::Comp(l, r, attr) => {
                // Inside a broken sequence, a breakable composition becomes a
                // hard line.
                if brk[i] && attr.brk == Break::Breakable {
                    nodes.push(LayoutNode::Line(out[l], out[r]))
                } else {
                    nodes.push(LayoutNode::Comp(out[l], out[r], attr))
                }
            }
        };
        out.push(id);
    }

    LayoutArena {
        nodes,
        root: out[arena.root],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::constructors::{comp, line, seq, text};
    use crate::compiler::passes::flatten::flatten;
    use crate::compiler::types::Pad;

    #[test]
    fn broken_seq_turns_comps_into_lines() {
        // seq(a + (b @ c)): the seq contains a hard line, so the wrapper drops
        // and the breakable comp becomes a line.
        let layout = seq(comp(
            text("a"),
            line(text("b"), text("c")),
            Pad::Padded,
            Break::Breakable,
        ));
        let (arena, text) = flatten(*layout);
        let out = resolve_breaks(&arena);
        let LayoutNode::Line(l, _) = out.nodes[out.root] else {
            panic!("expected the comp to become a line");
        };
        assert!(matches!(out.nodes[l], LayoutNode::Text(r) if r.slice(&text) == "a"));
    }

    #[test]
    fn unbroken_seq_keeps_wrapper_and_comps() {
        let layout = seq(comp(text("a"), text("b"), Pad::Padded, Break::Breakable));
        let (arena, _text) = flatten(*layout);
        let out = resolve_breaks(&arena);
        let LayoutNode::Seq(c) = out.nodes[out.root] else {
            panic!("expected the seq wrapper to survive");
        };
        assert!(matches!(out.nodes[c], LayoutNode::Comp(..)));
    }

    #[test]
    fn fixed_comp_survives_inside_broken_seq() {
        // The fixed composition must not become a line even inside a broken
        // sequence.
        let layout = seq(comp(
            comp(text("a"), text("b"), Pad::Padded, Break::Fixed),
            line(text("c"), text("d")),
            Pad::Padded,
            Break::Breakable,
        ));
        let (arena, _text) = flatten(*layout);
        let out = resolve_breaks(&arena);
        // Root is the outer comp turned line; its left is the fixed comp.
        let LayoutNode::Line(l, _) = out.nodes[out.root] else {
            panic!("expected the outer comp to become a line");
        };
        assert!(matches!(out.nodes[l], LayoutNode::Comp(..)));
    }
}
