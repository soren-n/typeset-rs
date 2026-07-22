//! flatten: Layout → LayoutArena (flatten the input tree)
//!
//! Lowers the public `Box`-recursive [`Layout`] tree into a flat postorder
//! arena (children precede parents). This is the pipeline's entry step and the
//! single place that walks owning boxes: children are taken out of their boxes
//! (leaving `Null` placeholders, so each box's own drop terminates in O(1))
//! and text is moved out of the tree — every later representation borrows it
//! from this arena. All subsequent passes fold flat arenas with plain loops.

use crate::compiler::types::{LayId, Layout, LayoutArena, LayoutNode};
use std::mem;

/// A unit of flattening work: visit a subtree, or build a parent node from
/// already-flattened children (their ids sit on the result stack).
enum Task {
    Visit(Box<Layout>),
    Fix,
    Grp,
    Seq,
    Nest,
    Pack,
    Line,
    Comp(crate::compiler::types::Attr),
}

pub fn flatten(layout: Box<Layout>) -> LayoutArena {
    fn push(nodes: &mut Vec<LayoutNode>, node: LayoutNode) -> LayId {
        let id = nodes.len() as LayId;
        nodes.push(node);
        id
    }
    let mut nodes: Vec<LayoutNode> = Vec::new();

    let mut tasks: Vec<Task> = vec![Task::Visit(layout)];
    let mut ids: Vec<LayId> = Vec::new();
    while let Some(task) = tasks.pop() {
        match task {
            Task::Visit(mut cur) => match &mut *cur {
                Layout::Null => {
                    let id = push(&mut nodes, LayoutNode::Null);
                    ids.push(id);
                }
                Layout::Text(data) => {
                    let id = push(&mut nodes, LayoutNode::Text(mem::take(data)));
                    ids.push(id);
                }
                Layout::Fix(child) => {
                    tasks.push(Task::Fix);
                    tasks.push(Task::Visit(mem::take(child)));
                }
                Layout::Grp(child) => {
                    tasks.push(Task::Grp);
                    tasks.push(Task::Visit(mem::take(child)));
                }
                Layout::Seq(child) => {
                    tasks.push(Task::Seq);
                    tasks.push(Task::Visit(mem::take(child)));
                }
                Layout::Nest(child) => {
                    tasks.push(Task::Nest);
                    tasks.push(Task::Visit(mem::take(child)));
                }
                Layout::Pack(child) => {
                    tasks.push(Task::Pack);
                    tasks.push(Task::Visit(mem::take(child)));
                }
                Layout::Line(left, right) => {
                    tasks.push(Task::Line);
                    tasks.push(Task::Visit(mem::take(right)));
                    tasks.push(Task::Visit(mem::take(left)));
                }
                Layout::Comp(left, right, attr) => {
                    tasks.push(Task::Comp(*attr));
                    tasks.push(Task::Visit(mem::take(right)));
                    tasks.push(Task::Visit(mem::take(left)));
                }
            },
            Task::Fix | Task::Grp | Task::Seq | Task::Nest | Task::Pack => {
                let child = ids.pop().expect("unary operand");
                let node = match task {
                    Task::Fix => LayoutNode::Fix(child),
                    Task::Grp => LayoutNode::Grp(child),
                    Task::Seq => LayoutNode::Seq(child),
                    Task::Nest => LayoutNode::Nest(child),
                    Task::Pack => LayoutNode::Pack(child),
                    _ => unreachable!(),
                };
                let id = push(&mut nodes, node);
                ids.push(id);
            }
            Task::Line => {
                let right = ids.pop().expect("line: right operand");
                let left = ids.pop().expect("line: left operand");
                let id = push(&mut nodes, LayoutNode::Line(left, right));
                ids.push(id);
            }
            Task::Comp(attr) => {
                let right = ids.pop().expect("comp: right operand");
                let left = ids.pop().expect("comp: left operand");
                let id = push(&mut nodes, LayoutNode::Comp(left, right, attr));
                ids.push(id);
            }
        }
    }
    let root = ids.pop().expect("flatten produced a root");
    assert!(ids.is_empty(), "flatten consumed every subtree");
    LayoutArena { nodes, root }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::constructors::{comp, nest, text};
    use crate::compiler::types::{Break, Pad};

    /// Deeper than a native-stack recursion could survive.
    const DEEP: usize = 50_000;

    #[test]
    fn flatten_is_postorder() {
        let layout = comp(text("a"), nest(text("b")), Pad::Padded, Break::Breakable);
        let arena = flatten(layout);
        // Postorder: a, b, Nest(b), Comp — the root is last.
        assert_eq!(arena.root as usize, arena.nodes.len() - 1);
        assert!(matches!(arena.nodes[0], LayoutNode::Text(ref s) if s == "a"));
        assert!(matches!(arena.nodes[1], LayoutNode::Text(ref s) if s == "b"));
        assert!(matches!(arena.nodes[2], LayoutNode::Nest(1)));
        assert!(matches!(arena.nodes[3], LayoutNode::Comp(0, 2, _)));
    }

    #[test]
    fn flatten_handles_deep_layout() {
        let mut layout = text("x");
        for _ in 0..DEEP {
            layout = comp(layout, text("y"), Pad::Unpadded, Break::Breakable);
        }
        let arena = flatten(layout);
        assert_eq!(arena.nodes.len(), 2 * DEEP + 1);
        assert_eq!(arena.root as usize, arena.nodes.len() - 1);
    }
}
