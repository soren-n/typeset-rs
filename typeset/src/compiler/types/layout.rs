//! The public input type: a [`Layout`] tree stored as a flat arena.

use super::arena::{Arena, Id, Range};

/// Whether a composition puts a space between its two operands when they share
/// a line — the padding axis of [`comp`](crate::comp).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Pad {
    /// No space between the operands (`foo` then `(` renders as `foo(`).
    Unpadded,
    /// A single space between the operands (`foo` then `bar` renders as
    /// `foo bar`).
    Padded,
}

/// Whether a composition may break across lines — the break axis of
/// [`comp`](crate::comp). `Fixed` is the composition-level analogue of wrapping
/// in [`fix`](crate::fix).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Break {
    /// The composition may move its right operand to the next line when it does
    /// not fit within the target width.
    Breakable,
    /// The composition never breaks; both operands always stay on one line.
    Fixed,
}

impl Pad {
    /// The columns a composition's padding occupies: one space or none.
    pub(crate) fn width(self) -> usize {
        match self {
            Pad::Unpadded => 0,
            Pad::Padded => 1,
        }
    }

    /// The padding two merged compositions share: padded if either is.
    pub(crate) fn merge(self, other: Pad) -> Pad {
        match (self, other) {
            (Pad::Unpadded, Pad::Unpadded) => Pad::Unpadded,
            _ => Pad::Padded,
        }
    }
}

/// The two axes of a composition: padding and breakability.
#[derive(Debug, Copy, Clone)]
pub(crate) struct Attr {
    pub(crate) pad: Pad,
    pub(crate) brk: Break,
}

pub(crate) type LayId = Id<LayoutNode>;

/// One node of a [`Layout`]: children are arena ids, text is a range into
/// the layout's text buffer.
#[derive(Debug, Copy, Clone)]
pub(crate) enum LayoutNode {
    Null,
    Text(Range<str>),
    Fix(LayId),
    Grp(LayId),
    Seq(LayId),
    Nest(LayId),
    Pack(LayId),
    Line(LayId, LayId),
    Comp(LayId, LayId, Attr),
}

impl LayoutNode {
    /// The same node with its child ids and text range shifted, for
    /// appending one arena onto another.
    fn offset(self, nodes: usize, text: usize) -> LayoutNode {
        let id = |id: LayId| Id::from_index(id.index() + nodes);
        match self {
            LayoutNode::Null => LayoutNode::Null,
            LayoutNode::Text(r) => LayoutNode::Text(Range::new(r.start() + text, r.end() + text)),
            LayoutNode::Fix(c) => LayoutNode::Fix(id(c)),
            LayoutNode::Grp(c) => LayoutNode::Grp(id(c)),
            LayoutNode::Seq(c) => LayoutNode::Seq(id(c)),
            LayoutNode::Nest(c) => LayoutNode::Nest(id(c)),
            LayoutNode::Pack(c) => LayoutNode::Pack(id(c)),
            LayoutNode::Line(l, r) => LayoutNode::Line(id(l), id(r)),
            LayoutNode::Comp(l, r, attr) => LayoutNode::Comp(id(l), id(r), attr),
        }
    }
}

/// A layout: the input to [`compile`](crate::compile). Built with the
/// constructor functions ([`text`](crate::text), [`comp`](crate::comp),
/// [`nest`](crate::nest), ...); never inspected directly.
///
/// Stored as a flat postorder arena — every node's children precede it and
/// the root is the last node — with all text in one buffer. Combining two
/// layouts appends the smaller arena onto the larger, so building a layout of
/// `n` nodes costs O(n log n) in the worst case and O(n) for the usual
/// left- or right-leaning chains. Being flat, a layout of any depth clones,
/// drops, and prints without recursion.
#[derive(Clone, Debug)]
pub struct Layout {
    pub(crate) nodes: Arena<LayoutNode>,
    pub(crate) text: String,
}

impl Layout {
    /// A single-node layout.
    pub(crate) fn leaf(node: LayoutNode) -> Layout {
        let mut nodes = Arena::with_capacity(1);
        nodes.push(node);
        Layout {
            nodes,
            text: String::new(),
        }
    }

    /// A text leaf.
    pub(crate) fn text(text: String) -> Layout {
        let mut nodes = Arena::with_capacity(1);
        nodes.push(LayoutNode::Text(Range::new(0, text.len())));
        Layout { nodes, text }
    }

    /// The root node's id: always the last node.
    pub(crate) fn root(&self) -> LayId {
        Id::from_index(self.nodes.len() - 1)
    }

    /// Wraps the whole layout in a unary node.
    pub(crate) fn unary(mut self, make: impl FnOnce(LayId) -> LayoutNode) -> Layout {
        let root = self.root();
        self.nodes.push(make(root));
        self
    }

    /// Joins two layouts under a binary node. The smaller arena is appended
    /// onto the larger (ids and text ranges shifted), then the parent is
    /// pushed, so both operands still precede it.
    pub(crate) fn binary(
        left: Layout,
        right: Layout,
        make: impl FnOnce(LayId, LayId) -> LayoutNode,
    ) -> Layout {
        let left_is_base = left.nodes.len() >= right.nodes.len();
        let (mut base, other) = if left_is_base {
            (left, right)
        } else {
            (right, left)
        };
        let base_root = base.root();
        let node_offset = base.nodes.len();
        let text_offset = base.text.len();
        base.text.push_str(&other.text);
        base.nodes
            .append(other.nodes, |node| node.offset(node_offset, text_offset));
        let other_root = base.root();
        let (l, r) = if left_is_base {
            (base_root, other_root)
        } else {
            (other_root, base_root)
        };
        base.nodes.push(make(l, r));
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::constructors::{comp, nest, text};

    /// Deeper than a native-stack recursion could survive.
    const DEEP: usize = 50_000;

    #[test]
    fn nodes_are_postorder_with_root_last() {
        let layout = comp(text("a"), nest(text("b")), Pad::Padded, Break::Breakable);
        let nodes = layout.nodes.as_slice();
        assert_eq!(nodes.len(), 4);
        let LayoutNode::Comp(l, r, _) = nodes[layout.root().index()] else {
            panic!("root is the comp");
        };
        assert!(matches!(nodes[l.index()], LayoutNode::Text(s) if s.slice(&layout.text) == "a"));
        let LayoutNode::Nest(c) = nodes[r.index()] else {
            panic!("right is the nest");
        };
        assert!(matches!(nodes[c.index()], LayoutNode::Text(s) if s.slice(&layout.text) == "b"));
    }

    #[test]
    fn right_leaning_chain_merges_small_into_large() {
        // Each step joins a one-node leaf with the growing chain; the chain
        // stays the base, so the whole build is linear.
        let mut layout = text("z");
        for _ in 0..DEEP {
            layout = comp(text("y"), layout, Pad::Unpadded, Break::Breakable);
        }
        assert_eq!(layout.nodes.len(), 2 * DEEP + 1);
        // The text buffer holds every leaf regardless of merge direction.
        assert_eq!(layout.text.len(), DEEP + 1);
        let LayoutNode::Comp(l, _, _) = layout.nodes[layout.root()] else {
            panic!("root is a comp");
        };
        assert!(matches!(layout.nodes[l], LayoutNode::Text(s) if s.slice(&layout.text) == "y"));
    }

    #[test]
    fn deep_layout_clones_drops_and_debugs_flat() {
        let mut layout = text("x");
        for _ in 0..DEEP {
            layout = nest(layout);
        }
        let cloned = layout.clone();
        assert_eq!(cloned.nodes.len(), DEEP + 1);
        let _ = format!("{:?}", cloned);
        drop(layout);
        drop(cloned);
    }
}
