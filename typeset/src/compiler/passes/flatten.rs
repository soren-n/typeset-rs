//! flatten: Layout → LayoutArena (flatten the input tree)
//!
//! Lowers the public `Box`-recursive [`Layout`] tree into a flat postorder
//! arena (children precede parents). This is the pipeline's entry step and the
//! single place that walks owning boxes: children are taken out of their boxes
//! (leaving `Null` placeholders, so each box's own drop terminates in O(1))
//! and text is moved out of the tree — every later representation borrows it
//! from this arena. All subsequent passes fold flat arenas with plain loops.

use crate::compiler::types::{Attr, LayId, Layout, LayoutArena, LayoutNode, push_node};
use std::mem;

/// A unit of flattening work: visit a subtree, or build a parent node from
/// already-flattened children (their ids sit on the result stack). A unary
/// parent carries its arena-node constructor directly.
///
/// `Visit` carries the `Layout` by value (moved out of its box with `Null`
/// left behind): taking the `Box` itself would allocate a placeholder box per
/// child (`Box::default()`), doubling the tree-teardown allocator traffic.
enum Task {
    Visit(Layout),
    Unary(fn(LayId) -> LayoutNode),
    Line,
    Comp(Attr),
}

/// Push a unary parent task, then its child; the child pops (and resolves)
/// first, so by the time the parent task pops its child id is on the result
/// stack.
fn visit_unary(tasks: &mut Vec<Task>, ctor: fn(LayId) -> LayoutNode, child: &mut Layout) {
    tasks.push(Task::Unary(ctor));
    tasks.push(Task::Visit(mem::replace(child, Layout::Null)));
}

pub fn flatten(layout: Layout) -> LayoutArena {
    let mut nodes: Vec<LayoutNode> = Vec::new();

    let mut tasks: Vec<Task> = vec![Task::Visit(layout)];
    let mut ids: Vec<LayId> = Vec::new();
    while let Some(task) = tasks.pop() {
        match task {
            Task::Visit(mut cur) => match &mut cur {
                Layout::Null => ids.push(push_node(&mut nodes, LayoutNode::Null)),
                Layout::Text(data) => {
                    ids.push(push_node(&mut nodes, LayoutNode::Text(mem::take(data))));
                }
                Layout::Fix(child) => visit_unary(&mut tasks, LayoutNode::Fix, child),
                Layout::Grp(child) => visit_unary(&mut tasks, LayoutNode::Grp, child),
                Layout::Seq(child) => visit_unary(&mut tasks, LayoutNode::Seq, child),
                Layout::Nest(child) => visit_unary(&mut tasks, LayoutNode::Nest, child),
                Layout::Pack(child) => visit_unary(&mut tasks, LayoutNode::Pack, child),
                Layout::Line(left, right) => {
                    tasks.push(Task::Line);
                    tasks.push(Task::Visit(mem::replace(&mut **right, Layout::Null)));
                    tasks.push(Task::Visit(mem::replace(&mut **left, Layout::Null)));
                }
                Layout::Comp(left, right, attr) => {
                    tasks.push(Task::Comp(*attr));
                    tasks.push(Task::Visit(mem::replace(&mut **right, Layout::Null)));
                    tasks.push(Task::Visit(mem::replace(&mut **left, Layout::Null)));
                }
            },
            Task::Unary(ctor) => {
                let child = ids.pop().expect("unary operand");
                ids.push(push_node(&mut nodes, ctor(child)));
            }
            Task::Line => {
                let right = ids.pop().expect("line: right operand");
                let left = ids.pop().expect("line: left operand");
                ids.push(push_node(&mut nodes, LayoutNode::Line(left, right)));
            }
            Task::Comp(attr) => {
                let right = ids.pop().expect("comp: right operand");
                let left = ids.pop().expect("comp: left operand");
                ids.push(push_node(&mut nodes, LayoutNode::Comp(left, right, attr)));
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
        let arena = flatten(*layout);
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
        let arena = flatten(*layout);
        assert_eq!(arena.nodes.len(), 2 * DEEP + 1);
        assert_eq!(arena.root as usize, arena.nodes.len() - 1);
    }
}
