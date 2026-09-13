//! resolve_breaks: LayoutArena → EdslDoc (collapse broken sequences)
//!
//! Resolves hard line breaks: a composition inside a broken sequence becomes a
//! `Line`, and a seq wrapper whose subtree already breaks is dropped (its
//! content is unconditionally broken anyway).
//!
//! The input is a flat postorder arena, so the pass is three plain loops:
//! 1. mark — forward (bottom-up): does each subtree contain a hard line break?
//! 2. spread — backward (top-down): is each node inside a broken sequence?
//! 3. build — forward: emit the Edsl arena, rewriting compositions to lines
//!    and dropping broken seq wrappers as the flags dictate.

use crate::compiler::types::{Arena, EdslDoc, EdslId, EdslNode, IdVec, LayoutArena, LayoutNode};

/// The returned [`EdslDoc`] borrows text from `text` (the layout text buffer),
/// not from `arena`, so the node arena is free to drop the moment this returns.
pub fn resolve_breaks<'t>(arena: &LayoutArena, text: &'t str) -> EdslDoc<'t> {
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

    // 3. build: emit the Edsl arena bottom-up.
    let mut nodes: Arena<EdslNode> = Arena::with_capacity(n);
    let mut out: IdVec<LayoutNode, EdslId> = IdVec::with_capacity(n);
    for (i, node) in arena.nodes.iter() {
        let id = match *node {
            LayoutNode::Null => nodes.push(EdslNode::Null),
            LayoutNode::Text(range) => nodes.push(EdslNode::Text(range.slice(text))),
            LayoutNode::Fix(c) => nodes.push(EdslNode::Fix(out[c])),
            LayoutNode::Grp(c) => nodes.push(EdslNode::Grp(out[c])),
            LayoutNode::Seq(c) => {
                // A sequence that already breaks is dropped: its content is
                // unconditionally broken (the flag spread takes care of that).
                if has_line[c] {
                    out[c]
                } else {
                    nodes.push(EdslNode::Seq(out[c]))
                }
            }
            LayoutNode::Nest(c) => nodes.push(EdslNode::Nest(out[c])),
            LayoutNode::Pack(c) => nodes.push(EdslNode::Pack(out[c])),
            LayoutNode::Line(l, r) => nodes.push(EdslNode::Line(out[l], out[r])),
            LayoutNode::Comp(l, r, attr) => {
                // Inside a broken sequence, a breakable composition becomes a
                // hard line.
                if brk[i] && !attr.brk.is_fixed() {
                    nodes.push(EdslNode::Line(out[l], out[r]))
                } else {
                    nodes.push(EdslNode::Comp(out[l], out[r], attr))
                }
            }
        };
        out.push(id);
    }

    EdslDoc {
        nodes,
        root: out[arena.root],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::constructors::{comp, line, seq, text};
    use crate::compiler::passes::flatten::flatten;
    use crate::compiler::types::{Break, Pad};

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
        let edsl = resolve_breaks(&arena, &text);
        let EdslNode::Line(l, _) = edsl.nodes[edsl.root] else {
            panic!("expected the comp to become a line");
        };
        assert!(matches!(edsl.nodes[l], EdslNode::Text("a")));
    }

    #[test]
    fn unbroken_seq_keeps_wrapper_and_comps() {
        let layout = seq(comp(text("a"), text("b"), Pad::Padded, Break::Breakable));
        let (arena, text) = flatten(*layout);
        let edsl = resolve_breaks(&arena, &text);
        let EdslNode::Seq(c) = edsl.nodes[edsl.root] else {
            panic!("expected the seq wrapper to survive");
        };
        assert!(matches!(edsl.nodes[c], EdslNode::Comp(..)));
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
        let (arena, text) = flatten(*layout);
        let edsl = resolve_breaks(&arena, &text);
        // Root is the outer comp turned line; its left is the fixed comp.
        let EdslNode::Line(l, _) = edsl.nodes[edsl.root] else {
            panic!("expected the outer comp to become a line");
        };
        assert!(matches!(edsl.nodes[l], EdslNode::Comp(..)));
    }
}
