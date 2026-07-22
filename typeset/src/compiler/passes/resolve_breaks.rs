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

use crate::compiler::types::{EdslDoc, EdslId, EdslNode, LayoutArena, LayoutNode};

pub fn resolve_breaks(arena: &LayoutArena) -> EdslDoc<'_> {
    let n = arena.nodes.len();

    // 1. mark: whether each subtree contains a hard line break. Wrappers pass
    // the flag through; a Line is one; a Comp has one if either operand does.
    let mut has_line = vec![false; n];
    for i in 0..n {
        has_line[i] = match arena.nodes[i] {
            LayoutNode::Null | LayoutNode::Text(_) => false,
            LayoutNode::Fix(c)
            | LayoutNode::Grp(c)
            | LayoutNode::Seq(c)
            | LayoutNode::Nest(c)
            | LayoutNode::Pack(c) => has_line[c as usize],
            LayoutNode::Line(..) => true,
            LayoutNode::Comp(l, r, _) => has_line[l as usize] || has_line[r as usize],
        };
    }

    // 2. spread: whether each node sits inside a broken sequence. Fix and grp
    // reset the context; a seq sets it to its own subtree's flag; everything
    // else passes it down. The root starts outside any sequence.
    let mut brk = vec![false; n];
    for i in (0..n).rev() {
        match arena.nodes[i] {
            LayoutNode::Null | LayoutNode::Text(_) => {}
            LayoutNode::Fix(c) | LayoutNode::Grp(c) => brk[c as usize] = false,
            LayoutNode::Seq(c) => brk[c as usize] = has_line[c as usize],
            LayoutNode::Nest(c) | LayoutNode::Pack(c) => brk[c as usize] = brk[i],
            LayoutNode::Line(l, r) | LayoutNode::Comp(l, r, _) => {
                brk[l as usize] = brk[i];
                brk[r as usize] = brk[i];
            }
        }
    }

    // 3. build: emit the Edsl arena bottom-up.
    fn push<'a>(nodes: &mut Vec<EdslNode<'a>>, node: EdslNode<'a>) -> EdslId {
        let id = nodes.len() as EdslId;
        nodes.push(node);
        id
    }
    let mut nodes: Vec<EdslNode> = Vec::with_capacity(n);
    let mut out: Vec<EdslId> = Vec::with_capacity(n);
    for (i, node) in arena.nodes.iter().enumerate() {
        let id = match node {
            LayoutNode::Null => push(&mut nodes, EdslNode::Null),
            LayoutNode::Text(data) => push(&mut nodes, EdslNode::Text(data)),
            LayoutNode::Fix(c) => push(&mut nodes, EdslNode::Fix(out[*c as usize])),
            LayoutNode::Grp(c) => push(&mut nodes, EdslNode::Grp(out[*c as usize])),
            LayoutNode::Seq(c) => {
                // A sequence that already breaks is dropped: its content is
                // unconditionally broken (the flag spread takes care of that).
                if has_line[*c as usize] {
                    out[*c as usize]
                } else {
                    push(&mut nodes, EdslNode::Seq(out[*c as usize]))
                }
            }
            LayoutNode::Nest(c) => push(&mut nodes, EdslNode::Nest(out[*c as usize])),
            LayoutNode::Pack(c) => push(&mut nodes, EdslNode::Pack(out[*c as usize])),
            LayoutNode::Line(l, r) => push(
                &mut nodes,
                EdslNode::Line(out[*l as usize], out[*r as usize]),
            ),
            LayoutNode::Comp(l, r, attr) => {
                // Inside a broken sequence, a breakable composition becomes a
                // hard line.
                if brk[i] && !attr.brk.is_fixed() {
                    push(
                        &mut nodes,
                        EdslNode::Line(out[*l as usize], out[*r as usize]),
                    )
                } else {
                    push(
                        &mut nodes,
                        EdslNode::Comp(out[*l as usize], out[*r as usize], *attr),
                    )
                }
            }
        };
        out.push(id);
    }

    EdslDoc {
        nodes,
        root: out[arena.root as usize],
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
        let arena = flatten(layout);
        let edsl = resolve_breaks(&arena);
        let EdslNode::Line(l, _) = edsl.nodes[edsl.root as usize] else {
            panic!("expected the comp to become a line");
        };
        assert!(matches!(edsl.nodes[l as usize], EdslNode::Text("a")));
    }

    #[test]
    fn unbroken_seq_keeps_wrapper_and_comps() {
        let layout = seq(comp(text("a"), text("b"), Pad::Padded, Break::Breakable));
        let arena = flatten(layout);
        let edsl = resolve_breaks(&arena);
        let EdslNode::Seq(c) = edsl.nodes[edsl.root as usize] else {
            panic!("expected the seq wrapper to survive");
        };
        assert!(matches!(edsl.nodes[c as usize], EdslNode::Comp(..)));
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
        let arena = flatten(layout);
        let edsl = resolve_breaks(&arena);
        // Root is the outer comp turned line; its left is the fixed comp.
        let EdslNode::Line(l, _) = edsl.nodes[edsl.root as usize] else {
            panic!("expected the outer comp to become a line");
        };
        assert!(matches!(edsl.nodes[l as usize], EdslNode::Comp(..)));
    }
}
