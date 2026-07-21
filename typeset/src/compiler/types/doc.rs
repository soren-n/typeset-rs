//! Final document representation - output of the compiler.
//!
//! `Doc` is a flat arena, not a `Box`-recursive tree. The document object graph
//! (the `Comp`/`Grp`/`Nest`/… nodes that used to be a tree of `Box<DocObj>`) is
//! stored in two flat `Vec`s — one for objects, one for fixed objects — and
//! children are referenced by arena index rather than by owning box. The spine
//! (`Empty`/`Break`/`Line`/`Eod`) was already linear and is a `Vec<Row>` in
//! document order.
//!
//! The point of the flat representation is that deep-safety is now *structural*
//! rather than hand-maintained: dropping or cloning a `Doc` is dropping/cloning
//! three `Vec`s of shallow records, which never recurses no matter how deeply
//! nested the document is. So `Clone` and `Drop` are derived, and the ~600 lines
//! of hand-rolled iterative `Clone`/`Drop` the old `Box`-tree needed are gone.
//!
//! `Display` and `Debug` still walk the object graph to print the historical
//! parenthesized / derived-looking forms, and those walks are driven by an
//! explicit index stack so that printing a deep document does not recurse on the
//! native stack either.

use std::fmt;

/// Index into a [`Doc`]'s object arena ([`Doc::objs`]).
pub(crate) type ObjId = u32;

/// Index into a [`Doc`]'s fixed-object arena ([`Doc::fixes`]).
pub(crate) type FixId = u32;

/// A node in the object arena. Children are arena indices, not owning boxes, so
/// a node is a shallow record and the whole arena drops/clones without recursion.
#[derive(Clone, Debug)]
pub(crate) enum ObjNode {
    Text(String),
    Fix(FixId),
    Grp(ObjId),
    Seq(ObjId),
    Nest(ObjId),
    Pack(u64, ObjId),
    Comp(ObjId, ObjId, bool),
}

/// A node in the fixed-object arena (the subset of objects that never break).
#[derive(Clone, Debug)]
pub(crate) enum FixNode {
    Text(String),
    Comp(FixId, FixId, bool),
}

/// One row of the document spine, in document order.
///
/// A `Line` row is always the last row (nothing follows a line); a document that
/// ends in `Eod` simply has no `Line` row. The spine is walked front-to-back by
/// the renderer, stopping at a `Line` or running off the end (`Eod`).
#[derive(Clone, Debug)]
pub(crate) enum Row {
    Empty,
    Break(ObjId),
    Line(ObjId),
}

/// Final document representation - output of the compiler.
///
/// A flat arena: the spine is a `Vec<Row>` and the object graph lives in two
/// index-linked `Vec`s. Callers never construct or inspect a `Doc`; they pass it
/// to [`render`](crate::render()). `Clone` and `Drop` are derived and structurally
/// deep-safe (they touch only flat `Vec`s), so no amount of document nesting can
/// overflow the stack when a `Doc` is cloned or freed.
#[derive(Clone)]
pub struct Doc {
    rows: Vec<Row>,
    objs: Vec<ObjNode>,
    fixes: Vec<FixNode>,
}

impl Doc {
    /// The spine rows, in document order.
    pub(crate) fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The object arena.
    pub(crate) fn objs(&self) -> &[ObjNode] {
        &self.objs
    }

    /// The fixed-object arena.
    pub(crate) fn fixes(&self) -> &[FixNode] {
        &self.fixes
    }
}

/// Appends object arena nodes and returns their indices while lowering into a
/// [`Doc`].
///
/// The final compiler pass ([`rescope`](crate::compiler::passes::rescope)) drives
/// this: it pushes each object/fixed-object node as it is built (children before
/// parents, so a parent's child indices always already exist) and collects the
/// spine rows separately, then calls [`finish`](DocBuilder::finish).
pub(crate) struct DocBuilder {
    objs: Vec<ObjNode>,
    fixes: Vec<FixNode>,
}

impl DocBuilder {
    pub(crate) fn new() -> Self {
        DocBuilder {
            objs: Vec::new(),
            fixes: Vec::new(),
        }
    }

    /// Append an object node and return its arena index.
    pub(crate) fn obj(&mut self, node: ObjNode) -> ObjId {
        let id = self.objs.len() as ObjId;
        self.objs.push(node);
        id
    }

    /// Append a fixed-object node and return its arena index.
    pub(crate) fn fix(&mut self, node: FixNode) -> FixId {
        let id = self.fixes.len() as FixId;
        self.fixes.push(node);
        id
    }

    /// Assemble the finished document from the collected spine rows.
    pub(crate) fn finish(self, rows: Vec<Row>) -> Doc {
        Doc {
            rows,
            objs: self.objs,
            fixes: self.fixes,
        }
    }
}

// Iterative Display / Debug
// -------------------------
// Both print the document object graph in its historical tree-shaped form (the
// parenthesized form for `Display`, the derived-looking form for `Debug`). A
// document can be arbitrarily deep, so each object walk is driven by an explicit
// task stack over arena indices rather than by recursion.

/// A token in an object/fixed-object print walk.
enum PrintTask {
    Obj(ObjId),
    Fix(FixId),
    Lit(&'static str),
    Owned(String),
}

/// Appends the parenthesized (`Display`) form of the object at `root` to `out`.
fn write_obj_display(out: &mut String, objs: &[ObjNode], fixes: &[FixNode], root: ObjId) {
    let mut stack: Vec<PrintTask> = vec![PrintTask::Obj(root)];
    while let Some(task) = stack.pop() {
        match task {
            PrintTask::Lit(s) => out.push_str(s),
            PrintTask::Owned(s) => out.push_str(&s),
            PrintTask::Obj(o) => match &objs[o as usize] {
                ObjNode::Text(data) => {
                    out.push_str("(Text \"");
                    out.push_str(data);
                    out.push_str("\")");
                }
                ObjNode::Fix(fix) => {
                    stack.push(PrintTask::Lit(")"));
                    stack.push(PrintTask::Fix(*fix));
                    stack.push(PrintTask::Lit("(Fix "));
                }
                ObjNode::Grp(obj1) => {
                    stack.push(PrintTask::Lit(")"));
                    stack.push(PrintTask::Obj(*obj1));
                    stack.push(PrintTask::Lit("(Grp "));
                }
                ObjNode::Seq(obj1) => {
                    stack.push(PrintTask::Lit(")"));
                    stack.push(PrintTask::Obj(*obj1));
                    stack.push(PrintTask::Lit("(Seq "));
                }
                ObjNode::Nest(obj1) => {
                    stack.push(PrintTask::Lit(")"));
                    stack.push(PrintTask::Obj(*obj1));
                    stack.push(PrintTask::Lit("(Nest "));
                }
                ObjNode::Pack(index, obj1) => {
                    stack.push(PrintTask::Lit(")"));
                    stack.push(PrintTask::Obj(*obj1));
                    stack.push(PrintTask::Owned(format!("(Pack {} ", index)));
                }
                ObjNode::Comp(left, right, pad) => {
                    stack.push(PrintTask::Owned(format!(" {})", pad)));
                    stack.push(PrintTask::Obj(*right));
                    stack.push(PrintTask::Lit(" "));
                    stack.push(PrintTask::Obj(*left));
                    stack.push(PrintTask::Lit("(Comp "));
                }
            },
            PrintTask::Fix(x) => match &fixes[x as usize] {
                FixNode::Text(data) => {
                    out.push_str("(Text \"");
                    out.push_str(data);
                    out.push_str("\")");
                }
                FixNode::Comp(left, right, pad) => {
                    stack.push(PrintTask::Owned(format!(" {})", pad)));
                    stack.push(PrintTask::Fix(*right));
                    stack.push(PrintTask::Lit(" "));
                    stack.push(PrintTask::Fix(*left));
                    stack.push(PrintTask::Lit("(Comp "));
                }
            },
        }
    }
}

impl fmt::Display for Doc {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // The spine is a linear list of rows; each object prints via the
        // iterative walk above. `Eod` is implicit (no `Line` row), so append it
        // unless the document ends in a line.
        let mut out = String::new();
        let mut line_terminated = false;
        for (i, row) in self.rows.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            match row {
                Row::Empty => out.push_str("Empty"),
                Row::Break(id) => {
                    out.push_str("Break ");
                    write_obj_display(&mut out, &self.objs, &self.fixes, *id);
                }
                Row::Line(id) => {
                    out.push_str("Line ");
                    write_obj_display(&mut out, &self.objs, &self.fixes, *id);
                    line_terminated = true;
                }
            }
        }
        if !line_terminated {
            if self.rows.is_empty() {
                out.push_str("Eod");
            } else {
                out.push_str("\nEod");
            }
        }
        write!(f, "{}", out)
    }
}

/// Appends the derived-looking (`Debug`) form of the object at `root` to `out`.
fn write_obj_debug(out: &mut String, objs: &[ObjNode], fixes: &[FixNode], root: ObjId) {
    let mut stack: Vec<PrintTask> = vec![PrintTask::Obj(root)];
    while let Some(task) = stack.pop() {
        match task {
            PrintTask::Lit(s) => out.push_str(s),
            PrintTask::Owned(s) => out.push_str(&s),
            PrintTask::Obj(o) => match &objs[o as usize] {
                ObjNode::Text(data) => out.push_str(&format!("Text({:?})", data)),
                ObjNode::Fix(fix) => {
                    stack.push(PrintTask::Lit(")"));
                    stack.push(PrintTask::Fix(*fix));
                    stack.push(PrintTask::Lit("Fix("));
                }
                ObjNode::Grp(obj1) => {
                    stack.push(PrintTask::Lit(")"));
                    stack.push(PrintTask::Obj(*obj1));
                    stack.push(PrintTask::Lit("Grp("));
                }
                ObjNode::Seq(obj1) => {
                    stack.push(PrintTask::Lit(")"));
                    stack.push(PrintTask::Obj(*obj1));
                    stack.push(PrintTask::Lit("Seq("));
                }
                ObjNode::Nest(obj1) => {
                    stack.push(PrintTask::Lit(")"));
                    stack.push(PrintTask::Obj(*obj1));
                    stack.push(PrintTask::Lit("Nest("));
                }
                ObjNode::Pack(index, obj1) => {
                    stack.push(PrintTask::Lit(")"));
                    stack.push(PrintTask::Obj(*obj1));
                    stack.push(PrintTask::Owned(format!("Pack({}, ", index)));
                }
                ObjNode::Comp(left, right, pad) => {
                    stack.push(PrintTask::Owned(format!(", {})", pad)));
                    stack.push(PrintTask::Obj(*right));
                    stack.push(PrintTask::Lit(", "));
                    stack.push(PrintTask::Obj(*left));
                    stack.push(PrintTask::Lit("Comp("));
                }
            },
            PrintTask::Fix(x) => match &fixes[x as usize] {
                FixNode::Text(data) => out.push_str(&format!("Text({:?})", data)),
                FixNode::Comp(left, right, pad) => {
                    stack.push(PrintTask::Owned(format!(", {})", pad)));
                    stack.push(PrintTask::Fix(*right));
                    stack.push(PrintTask::Lit(", "));
                    stack.push(PrintTask::Fix(*left));
                    stack.push(PrintTask::Lit("Comp("));
                }
            },
        }
    }
}

impl fmt::Debug for Doc {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // Reproduce byte-for-byte the output the old `Box`-recursive `Doc`'s
        // derived `Debug` produced: the spine nests each tail inside its parent
        // (`Break(obj, Line(obj))`, `Empty(Eod)`, …). The flat spine is walked
        // in order, opening one paren per `Empty`/`Break` (closed after the
        // whole tail) and closing `Line` (the terminal row) itself.
        let mut out = String::new();
        let mut opens = 0usize;
        let mut line_terminated = false;
        for row in &self.rows {
            match row {
                Row::Empty => {
                    out.push_str("Empty(");
                    opens += 1;
                }
                Row::Break(id) => {
                    out.push_str("Break(");
                    write_obj_debug(&mut out, &self.objs, &self.fixes, *id);
                    out.push_str(", ");
                    opens += 1;
                }
                Row::Line(id) => {
                    out.push_str("Line(");
                    write_obj_debug(&mut out, &self.objs, &self.fixes, *id);
                    out.push(')');
                    line_terminated = true;
                }
            }
        }
        if !line_terminated {
            out.push_str("Eod");
        }
        for _ in 0..opens {
            out.push(')');
        }
        f.write_str(&out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Depth chosen far past where a recursive drop/print aborted (~400-2000).
    const DEEP: usize = 50_000;

    /// Builds a single-line document whose object is `Nest^depth(Text("x"))`.
    fn deep_nest_line(depth: usize) -> Doc {
        let mut b = DocBuilder::new();
        let mut id = b.obj(ObjNode::Text("x".to_string()));
        for _ in 0..depth {
            id = b.obj(ObjNode::Nest(id));
        }
        b.finish(vec![Row::Line(id)])
    }

    #[test]
    fn deep_doc_drops_without_overflow() {
        // Structural now: dropping three `Vec`s never recurses. Kept as a guard.
        let doc = deep_nest_line(DEEP);
        drop(doc);
    }

    #[test]
    fn deep_doc_clones_without_overflow() {
        let doc = deep_nest_line(DEEP);
        let cloned = doc.clone();
        assert_eq!(format!("{}", cloned), format!("{}", doc));
    }

    #[test]
    fn clone_matches_original_shape() {
        // Break(Comp(Text "a", Fix(Comp(Text "c", Text "d", true)), false),
        //       Line(Pack(7, Text "z")))
        let mut b = DocBuilder::new();
        let a = b.obj(ObjNode::Text("a".to_string()));
        let c = b.fix(FixNode::Text("c".to_string()));
        let d = b.fix(FixNode::Text("d".to_string()));
        let fix_comp = b.fix(FixNode::Comp(c, d, true));
        let fix_obj = b.obj(ObjNode::Fix(fix_comp));
        let comp = b.obj(ObjNode::Comp(a, fix_obj, false));
        let z = b.obj(ObjNode::Text("z".to_string()));
        let pack = b.obj(ObjNode::Pack(7, z));
        let doc = b.finish(vec![Row::Break(comp), Row::Line(pack)]);
        assert_eq!(format!("{}", doc.clone()), format!("{}", doc));
    }

    #[test]
    fn deep_display_is_iterative() {
        let doc = deep_nest_line(DEEP);
        let s = format!("{}", doc);
        assert!(s.starts_with("Line (Nest "));
        assert!(s.contains("(Text \"x\")"));
        assert!(s.ends_with(')'));
        assert_eq!(s.matches("(Nest ").count(), DEEP);
    }

    #[test]
    fn display_format_matches_expected() {
        // Break(Comp(Text "a", Grp(Text "b"), true),
        //       Line(Fix(Comp(Text "c", Text "d", false))))
        let mut b = DocBuilder::new();
        let a = b.obj(ObjNode::Text("a".to_string()));
        let bx = b.obj(ObjNode::Text("b".to_string()));
        let grp = b.obj(ObjNode::Grp(bx));
        let comp = b.obj(ObjNode::Comp(a, grp, true));
        let c = b.fix(FixNode::Text("c".to_string()));
        let d = b.fix(FixNode::Text("d".to_string()));
        let fix_comp = b.fix(FixNode::Comp(c, d, false));
        let fix_obj = b.obj(ObjNode::Fix(fix_comp));
        let doc = b.finish(vec![Row::Break(comp), Row::Line(fix_obj)]);
        let expected = "Break (Comp (Text \"a\") (Grp (Text \"b\")) true)\n\
                        Line (Fix (Comp (Text \"c\") (Text \"d\") false))";
        assert_eq!(format!("{}", doc), expected);
    }

    #[test]
    fn display_eod_terminated_spine() {
        // Break(Text "a", Empty(Eod)) prints the trailing Eod.
        let mut b = DocBuilder::new();
        let a = b.obj(ObjNode::Text("a".to_string()));
        let doc = b.finish(vec![Row::Break(a), Row::Empty]);
        assert_eq!(format!("{}", doc), "Break (Text \"a\")\nEmpty\nEod");
    }

    #[test]
    fn debug_format_matches_derived() {
        // Byte-for-byte the output the old `#[derive(Debug)]` produced.
        // Break(Comp(Text "a", Pack(7, Fix(Comp(Text "c", Text "d", true))), false),
        //       Line(Grp(Text "b")))
        let mut b = DocBuilder::new();
        let a = b.obj(ObjNode::Text("a".to_string()));
        let c = b.fix(FixNode::Text("c".to_string()));
        let d = b.fix(FixNode::Text("d".to_string()));
        let fix_comp = b.fix(FixNode::Comp(c, d, true));
        let fix_obj = b.obj(ObjNode::Fix(fix_comp));
        let pack = b.obj(ObjNode::Pack(7, fix_obj));
        let comp = b.obj(ObjNode::Comp(a, pack, false));
        let bx = b.obj(ObjNode::Text("b".to_string()));
        let grp = b.obj(ObjNode::Grp(bx));
        let doc = b.finish(vec![Row::Break(comp), Row::Line(grp)]);
        let expected = "Break(Comp(Text(\"a\"), \
                        Pack(7, Fix(Comp(Text(\"c\"), Text(\"d\"), true))), false), \
                        Line(Grp(Text(\"b\"))))";
        assert_eq!(format!("{:?}", doc), expected);
    }

    #[test]
    fn deep_debug_is_iterative() {
        let doc = deep_nest_line(DEEP);
        let s = format!("{:?}", doc);
        assert!(s.starts_with("Line(Nest("));
        assert_eq!(s.matches("Nest(").count(), DEEP);
        assert!(s.contains("Text(\"x\")"));
    }
}
