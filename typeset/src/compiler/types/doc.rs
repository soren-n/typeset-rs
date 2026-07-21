use super::traversal::DismantleTree;
use std::fmt;
use std::mem;

// `Clone`, `Debug`, and `Display` are implemented iteratively below rather than
// derived: a derived (recursive) impl would overflow the native stack on deep
// documents, the same hazard the iterative `Drop` and renderer avoid.

/// Final document representation - output of the compiler
pub enum Doc {
    Eod,
    Empty(Box<Doc>),
    Break(Box<DocObj>, Box<Doc>),
    Line(Box<DocObj>),
}

pub enum DocObj {
    Text(String),
    Fix(Box<DocObjFix>),
    Grp(Box<DocObj>),
    Seq(Box<DocObj>),
    Nest(Box<DocObj>),
    Pack(u64, Box<DocObj>),
    Comp(Box<DocObj>, Box<DocObj>, bool),
}

pub enum DocObjFix {
    Text(String),
    Comp(Box<DocObjFix>, Box<DocObjFix>, bool),
}

// Iterative destructors
// ---------------------
// The default (compiler-generated) drop recurses down the `Box` chain, so a
// deeply nested document would exhaust the native stack when it is freed — the
// same failure mode the iterative compiler passes and renderer exist to avoid.
// Each `Drop` below dismantles its tree onto a heap-allocated worklist instead:
// every box that actually goes out of scope has already had its same-typed
// children moved onto the worklist (replaced with leaves), so its own recursive
// drop terminates in O(1). Cross-type children (a `DocObj`'s `DocObjFix`, a
// `Doc`'s `DocObj`) are freed by that other type's iterative `Drop`.
//
// Implementing `Drop` means these types can no longer be destructured by value
// (partial moves are forbidden); every consumer borrows instead — see the
// renderer and the `Display` impl below.

// Each `dismantle` moves the node's same-typed children onto the worklist
// (moving them out of their `Box`, freeing it, and leaving a leaf placeholder in
// the parent) and leaves any cross-type child in place to be freed by that
// type's own iterative `Drop`. The shared worklist driver is [`DismantleTree`].

impl DismantleTree for Doc {
    fn dismantle(&mut self, stack: &mut Vec<Self>) {
        match self {
            Doc::Eod | Doc::Line(_) => {}
            Doc::Empty(doc1) | Doc::Break(_, doc1) => {
                stack.push(*mem::replace(doc1, Box::new(Doc::Eod)));
            }
        }
    }
}

impl Drop for Doc {
    fn drop(&mut self) {
        self.drain();
    }
}

impl DismantleTree for DocObj {
    fn dismantle(&mut self, stack: &mut Vec<Self>) {
        match self {
            DocObj::Text(_) | DocObj::Fix(_) => {}
            DocObj::Grp(obj1) | DocObj::Seq(obj1) | DocObj::Nest(obj1) | DocObj::Pack(_, obj1) => {
                stack.push(*mem::replace(obj1, Box::new(DocObj::Text(String::new()))));
            }
            DocObj::Comp(left, right, _) => {
                stack.push(*mem::replace(left, Box::new(DocObj::Text(String::new()))));
                stack.push(*mem::replace(right, Box::new(DocObj::Text(String::new()))));
            }
        }
    }
}

impl Drop for DocObj {
    fn drop(&mut self) {
        self.drain();
    }
}

impl DismantleTree for DocObjFix {
    fn dismantle(&mut self, stack: &mut Vec<Self>) {
        match self {
            DocObjFix::Text(_) => {}
            DocObjFix::Comp(left, right, _) => {
                stack.push(*mem::replace(
                    left,
                    Box::new(DocObjFix::Text(String::new())),
                ));
                stack.push(*mem::replace(
                    right,
                    Box::new(DocObjFix::Text(String::new())),
                ));
            }
        }
    }
}

impl Drop for DocObjFix {
    fn drop(&mut self) {
        self.drain();
    }
}

// Iterative Clone
// ---------------
// Deep-copy each tree bottom-up with task/result stacks (the same shape as the
// `move_to_heap` pass), so cloning a deep document — e.g. the documented
// `render(doc.clone(), ...)` pattern for re-rendering at multiple widths — runs
// in constant native stack instead of overflowing.

fn clone_fix(fix: &DocObjFix) -> Box<DocObjFix> {
    enum Task<'a> {
        Visit(&'a DocObjFix),
        Comp(bool),
    }
    let mut tasks: Vec<Task> = vec![Task::Visit(fix)];
    let mut out: Vec<Box<DocObjFix>> = Vec::new();
    while let Some(task) = tasks.pop() {
        match task {
            Task::Visit(f) => match f {
                DocObjFix::Text(data) => out.push(Box::new(DocObjFix::Text(data.clone()))),
                DocObjFix::Comp(left, right, pad) => {
                    tasks.push(Task::Comp(*pad));
                    tasks.push(Task::Visit(right));
                    tasks.push(Task::Visit(left));
                }
            },
            Task::Comp(pad) => {
                let right = out.pop().expect("fix comp: right operand");
                let left = out.pop().expect("fix comp: left operand");
                out.push(Box::new(DocObjFix::Comp(left, right, pad)));
            }
        }
    }
    out.pop().expect("fix clone produced no result")
}

fn clone_obj(obj: &DocObj) -> Box<DocObj> {
    enum Task<'a> {
        Visit(&'a DocObj),
        Grp,
        Seq,
        Nest,
        Pack(u64),
        Comp(bool),
    }
    let mut tasks: Vec<Task> = vec![Task::Visit(obj)];
    let mut out: Vec<Box<DocObj>> = Vec::new();
    while let Some(task) = tasks.pop() {
        match task {
            Task::Visit(o) => match o {
                DocObj::Text(data) => out.push(Box::new(DocObj::Text(data.clone()))),
                DocObj::Fix(fix) => out.push(Box::new(DocObj::Fix(clone_fix(fix)))),
                DocObj::Grp(obj1) => {
                    tasks.push(Task::Grp);
                    tasks.push(Task::Visit(obj1));
                }
                DocObj::Seq(obj1) => {
                    tasks.push(Task::Seq);
                    tasks.push(Task::Visit(obj1));
                }
                DocObj::Nest(obj1) => {
                    tasks.push(Task::Nest);
                    tasks.push(Task::Visit(obj1));
                }
                DocObj::Pack(index, obj1) => {
                    tasks.push(Task::Pack(*index));
                    tasks.push(Task::Visit(obj1));
                }
                DocObj::Comp(left, right, pad) => {
                    tasks.push(Task::Comp(*pad));
                    tasks.push(Task::Visit(right));
                    tasks.push(Task::Visit(left));
                }
            },
            Task::Grp => {
                let inner = out.pop().expect("grp operand");
                out.push(Box::new(DocObj::Grp(inner)));
            }
            Task::Seq => {
                let inner = out.pop().expect("seq operand");
                out.push(Box::new(DocObj::Seq(inner)));
            }
            Task::Nest => {
                let inner = out.pop().expect("nest operand");
                out.push(Box::new(DocObj::Nest(inner)));
            }
            Task::Pack(index) => {
                let inner = out.pop().expect("pack operand");
                out.push(Box::new(DocObj::Pack(index, inner)));
            }
            Task::Comp(pad) => {
                let right = out.pop().expect("comp: right operand");
                let left = out.pop().expect("comp: left operand");
                out.push(Box::new(DocObj::Comp(left, right, pad)));
            }
        }
    }
    out.pop().expect("obj clone produced no result")
}

fn clone_doc(doc: &Doc) -> Box<Doc> {
    let mut spine: Vec<&Doc> = Vec::new();
    let mut node = doc;
    loop {
        spine.push(node);
        match node {
            Doc::Eod | Doc::Line(_) => break,
            Doc::Empty(doc1) | Doc::Break(_, doc1) => node = doc1,
        }
    }
    let mut acc: Option<Box<Doc>> = None;
    for node in spine.into_iter().rev() {
        let built = match node {
            Doc::Eod => Box::new(Doc::Eod),
            Doc::Line(obj) => Box::new(Doc::Line(clone_obj(obj))),
            Doc::Empty(_) => Box::new(Doc::Empty(acc.take().expect("empty: tail"))),
            Doc::Break(obj, _) => {
                Box::new(Doc::Break(clone_obj(obj), acc.take().expect("break: tail")))
            }
        };
        acc = Some(built);
    }
    acc.expect("empty spine")
}

impl Clone for Doc {
    fn clone(&self) -> Self {
        *clone_doc(self)
    }
}

impl Clone for DocObj {
    fn clone(&self) -> Self {
        *clone_obj(self)
    }
}

impl Clone for DocObjFix {
    fn clone(&self) -> Self {
        *clone_fix(self)
    }
}

/// Serialize a document object to the parenthesized debug form, iteratively.
///
/// Borrows the object (a `Drop`-carrying type cannot be moved out of) and walks
/// it with an explicit task stack so arbitrarily deep objects print without
/// recursing on the native stack.
fn print_obj(obj: &DocObj) -> String {
    enum Task<'a> {
        Obj(&'a DocObj),
        Fix(&'a DocObjFix),
        Lit(&'static str),
        Owned(String),
    }
    let mut out = String::new();
    let mut stack: Vec<Task> = vec![Task::Obj(obj)];
    while let Some(task) = stack.pop() {
        match task {
            Task::Lit(s) => out.push_str(s),
            Task::Owned(s) => out.push_str(&s),
            Task::Obj(o) => match o {
                DocObj::Text(data) => out.push_str(&format!("(Text \"{}\")", data)),
                DocObj::Fix(obj1) => {
                    stack.push(Task::Lit(")"));
                    stack.push(Task::Fix(obj1));
                    stack.push(Task::Lit("(Fix "));
                }
                DocObj::Grp(obj1) => {
                    stack.push(Task::Lit(")"));
                    stack.push(Task::Obj(obj1));
                    stack.push(Task::Lit("(Grp "));
                }
                DocObj::Seq(obj1) => {
                    stack.push(Task::Lit(")"));
                    stack.push(Task::Obj(obj1));
                    stack.push(Task::Lit("(Seq "));
                }
                DocObj::Nest(obj1) => {
                    stack.push(Task::Lit(")"));
                    stack.push(Task::Obj(obj1));
                    stack.push(Task::Lit("(Nest "));
                }
                DocObj::Pack(index, obj1) => {
                    stack.push(Task::Lit(")"));
                    stack.push(Task::Obj(obj1));
                    stack.push(Task::Owned(format!("(Pack {} ", index)));
                }
                DocObj::Comp(left, right, pad) => {
                    stack.push(Task::Owned(format!(" {})", pad)));
                    stack.push(Task::Obj(right));
                    stack.push(Task::Lit(" "));
                    stack.push(Task::Obj(left));
                    stack.push(Task::Lit("(Comp "));
                }
            },
            Task::Fix(f) => match f {
                DocObjFix::Text(data) => out.push_str(&format!("(Text \"{}\")", data)),
                DocObjFix::Comp(left, right, pad) => {
                    stack.push(Task::Owned(format!(" {})", pad)));
                    stack.push(Task::Fix(right));
                    stack.push(Task::Lit(" "));
                    stack.push(Task::Fix(left));
                    stack.push(Task::Lit("(Comp "));
                }
            },
        }
    }
    out
}

impl fmt::Display for Doc {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        // The document spine is a linear list, so it prints with a plain loop;
        // objects print via the iterative `print_obj` above.
        let mut out = String::new();
        let mut node = self;
        loop {
            match node {
                Doc::Eod => {
                    out.push_str("Eod");
                    break;
                }
                Doc::Empty(doc1) => {
                    out.push_str("Empty\n");
                    node = doc1;
                }
                Doc::Break(obj, doc1) => {
                    out.push_str("Break ");
                    out.push_str(&print_obj(obj));
                    out.push('\n');
                    node = doc1;
                }
                Doc::Line(obj) => {
                    out.push_str("Line ");
                    out.push_str(&print_obj(obj));
                    break;
                }
            }
        }
        write!(f, "{}", out)
    }
}

// Iterative Debug
// ---------------
// A derived `Debug` recurses down the `Box` chain and overflows the native stack
// on deep documents — the same hazard every other trait here avoids. These impls
// reproduce the derived (non-alternate) output byte-for-byte, driven by an
// explicit task stack shared across the `Doc`/`DocObj`/`DocObjFix` family. The
// alternate (`{:#?}`) indented form is not reproduced; it falls back to the same
// compact form.

enum DebugTask<'a> {
    Doc(&'a Doc),
    Obj(&'a DocObj),
    Fix(&'a DocObjFix),
    Lit(&'static str),
    Owned(String),
}

fn debug_doc_family(f: &mut fmt::Formatter, start: DebugTask) -> fmt::Result {
    let mut stack: Vec<DebugTask> = vec![start];
    while let Some(task) = stack.pop() {
        match task {
            DebugTask::Lit(s) => f.write_str(s)?,
            DebugTask::Owned(s) => f.write_str(&s)?,
            DebugTask::Doc(d) => match d {
                Doc::Eod => f.write_str("Eod")?,
                Doc::Empty(doc1) => {
                    stack.push(DebugTask::Lit(")"));
                    stack.push(DebugTask::Doc(doc1));
                    stack.push(DebugTask::Lit("Empty("));
                }
                Doc::Break(obj, doc1) => {
                    stack.push(DebugTask::Lit(")"));
                    stack.push(DebugTask::Doc(doc1));
                    stack.push(DebugTask::Lit(", "));
                    stack.push(DebugTask::Obj(obj));
                    stack.push(DebugTask::Lit("Break("));
                }
                Doc::Line(obj) => {
                    stack.push(DebugTask::Lit(")"));
                    stack.push(DebugTask::Obj(obj));
                    stack.push(DebugTask::Lit("Line("));
                }
            },
            DebugTask::Obj(o) => match o {
                DocObj::Text(data) => f.write_str(&format!("Text({:?})", data))?,
                DocObj::Fix(obj1) => {
                    stack.push(DebugTask::Lit(")"));
                    stack.push(DebugTask::Fix(obj1));
                    stack.push(DebugTask::Lit("Fix("));
                }
                DocObj::Grp(obj1) => {
                    stack.push(DebugTask::Lit(")"));
                    stack.push(DebugTask::Obj(obj1));
                    stack.push(DebugTask::Lit("Grp("));
                }
                DocObj::Seq(obj1) => {
                    stack.push(DebugTask::Lit(")"));
                    stack.push(DebugTask::Obj(obj1));
                    stack.push(DebugTask::Lit("Seq("));
                }
                DocObj::Nest(obj1) => {
                    stack.push(DebugTask::Lit(")"));
                    stack.push(DebugTask::Obj(obj1));
                    stack.push(DebugTask::Lit("Nest("));
                }
                DocObj::Pack(index, obj1) => {
                    stack.push(DebugTask::Lit(")"));
                    stack.push(DebugTask::Obj(obj1));
                    stack.push(DebugTask::Owned(format!("Pack({}, ", index)));
                }
                DocObj::Comp(left, right, pad) => {
                    stack.push(DebugTask::Owned(format!(", {})", pad)));
                    stack.push(DebugTask::Obj(right));
                    stack.push(DebugTask::Lit(", "));
                    stack.push(DebugTask::Obj(left));
                    stack.push(DebugTask::Lit("Comp("));
                }
            },
            DebugTask::Fix(x) => match x {
                DocObjFix::Text(data) => f.write_str(&format!("Text({:?})", data))?,
                DocObjFix::Comp(left, right, pad) => {
                    stack.push(DebugTask::Owned(format!(", {})", pad)));
                    stack.push(DebugTask::Fix(right));
                    stack.push(DebugTask::Lit(", "));
                    stack.push(DebugTask::Fix(left));
                    stack.push(DebugTask::Lit("Comp("));
                }
            },
        }
    }
    Ok(())
}

impl fmt::Debug for Doc {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        debug_doc_family(f, DebugTask::Doc(self))
    }
}

impl fmt::Debug for DocObj {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        debug_doc_family(f, DebugTask::Obj(self))
    }
}

impl fmt::Debug for DocObjFix {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        debug_doc_family(f, DebugTask::Fix(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Depth chosen far past where the recursive drop/print aborted (~400-2000).
    const DEEP: usize = 50_000;

    #[test]
    fn deep_docobj_drops_without_overflow() {
        let mut obj = Box::new(DocObj::Text("x".to_string()));
        for _ in 0..DEEP {
            obj = Box::new(DocObj::Nest(obj));
        }
        drop(obj);
    }

    #[test]
    fn deep_docobj_comp_drops_without_overflow() {
        // Left-deep composition tree exercises the two-child drop path.
        let mut obj = Box::new(DocObj::Text("x".to_string()));
        for _ in 0..DEEP {
            obj = Box::new(DocObj::Comp(
                obj,
                Box::new(DocObj::Text("y".to_string())),
                false,
            ));
        }
        drop(obj);
    }

    #[test]
    fn deep_doc_spine_drops_without_overflow() {
        let mut doc = Box::new(Doc::Eod);
        for _ in 0..DEEP {
            doc = Box::new(Doc::Empty(doc));
        }
        drop(doc);
    }

    #[test]
    fn deep_docobjfix_drops_without_overflow() {
        let mut fix = Box::new(DocObjFix::Text("x".to_string()));
        for _ in 0..DEEP {
            fix = Box::new(DocObjFix::Comp(
                fix,
                Box::new(DocObjFix::Text("y".to_string())),
                false,
            ));
        }
        drop(fix);
    }

    #[test]
    fn deep_clone_is_iterative() {
        // A Break spine of Nest objects exercises both spine and object cloning.
        let mut doc = Box::new(Doc::Eod);
        for _ in 0..DEEP {
            let mut obj = Box::new(DocObj::Text("x".to_string()));
            for _ in 0..2 {
                obj = Box::new(DocObj::Nest(obj));
            }
            doc = Box::new(Doc::Break(obj, doc));
        }
        let cloned = doc.clone();
        // Clone is structurally identical to the original.
        assert_eq!(format!("{}", *cloned), format!("{}", *doc));
    }

    #[test]
    fn deep_object_clone_is_iterative() {
        let mut obj = Box::new(DocObj::Text("x".to_string()));
        for _ in 0..DEEP {
            obj = Box::new(DocObj::Nest(obj));
        }
        let cloned = obj.clone();
        assert_eq!(
            format!("{}", Doc::Line(cloned)),
            format!("{}", Doc::Line(obj))
        );
    }

    #[test]
    fn clone_matches_original_shape() {
        let doc = Doc::Break(
            Box::new(DocObj::Comp(
                Box::new(DocObj::Text("a".to_string())),
                Box::new(DocObj::Fix(Box::new(DocObjFix::Comp(
                    Box::new(DocObjFix::Text("c".to_string())),
                    Box::new(DocObjFix::Text("d".to_string())),
                    true,
                )))),
                false,
            )),
            Box::new(Doc::Line(Box::new(DocObj::Pack(
                7,
                Box::new(DocObj::Text("z".to_string())),
            )))),
        );
        assert_eq!(format!("{}", doc.clone()), format!("{}", doc));
    }

    #[test]
    fn deep_display_is_iterative() {
        let mut obj = Box::new(DocObj::Text("x".to_string()));
        for _ in 0..DEEP {
            obj = Box::new(DocObj::Nest(obj));
        }
        let doc = Doc::Line(obj);
        let s = format!("{}", doc);
        assert!(s.starts_with("Line (Nest "));
        // Innermost text, then one closing paren per Nest wrapper.
        assert!(s.contains("(Text \"x\")"));
        assert!(s.ends_with(')'));
        assert_eq!(s.matches("(Nest ").count(), DEEP);
    }

    #[test]
    fn display_format_matches_expected() {
        let doc = Doc::Break(
            Box::new(DocObj::Comp(
                Box::new(DocObj::Text("a".to_string())),
                Box::new(DocObj::Grp(Box::new(DocObj::Text("b".to_string())))),
                true,
            )),
            Box::new(Doc::Line(Box::new(DocObj::Fix(Box::new(DocObjFix::Comp(
                Box::new(DocObjFix::Text("c".to_string())),
                Box::new(DocObjFix::Text("d".to_string())),
                false,
            )))))),
        );
        let expected = "Break (Comp (Text \"a\") (Grp (Text \"b\")) true)\n\
                        Line (Fix (Comp (Text \"c\") (Text \"d\") false))";
        assert_eq!(format!("{}", doc), expected);
    }

    #[test]
    fn debug_format_matches_derived() {
        // Byte-for-byte the output the old `#[derive(Debug)]` produced.
        let doc = Doc::Break(
            Box::new(DocObj::Comp(
                Box::new(DocObj::Text("a".to_string())),
                Box::new(DocObj::Pack(
                    7,
                    Box::new(DocObj::Fix(Box::new(DocObjFix::Comp(
                        Box::new(DocObjFix::Text("c".to_string())),
                        Box::new(DocObjFix::Text("d".to_string())),
                        true,
                    )))),
                )),
                false,
            )),
            Box::new(Doc::Line(Box::new(DocObj::Grp(Box::new(DocObj::Text(
                "b".to_string(),
            )))))),
        );
        let expected = "Break(Comp(Text(\"a\"), \
                        Pack(7, Fix(Comp(Text(\"c\"), Text(\"d\"), true))), false), \
                        Line(Grp(Text(\"b\"))))";
        assert_eq!(format!("{:?}", doc), expected);
    }

    #[test]
    fn deep_debug_is_iterative() {
        let mut obj = Box::new(DocObj::Text("x".to_string()));
        for _ in 0..DEEP {
            obj = Box::new(DocObj::Nest(obj));
        }
        let doc = Doc::Line(obj);
        let s = format!("{:?}", doc);
        assert!(s.starts_with("Line(Nest("));
        assert_eq!(s.matches("Nest(").count(), DEEP);
        assert!(s.contains("Text(\"x\")"));
    }
}
