//! Pass 10: FinalDoc → Doc (move to heap)
//!
//! Converts the bump-allocated [`FinalDoc`] tree into an owned heap [`Doc`].
//! Every traversal is iterative: object subtrees are rebuilt bottom-up with an
//! explicit task/result stack, and the document spine (a linear list) is walked
//! into a `Vec` and folded from the tail. A deeply nested layout therefore
//! converts with a constant native stack.

use crate::compiler::types::{Doc, DocObj, DocObjFix, FinalDoc, FinalDocObj, FinalDocObjFix};

/// Rebuild a fixed object bottom-up (iterative).
fn _convert_fix(fix: &FinalDocObjFix) -> Box<DocObjFix> {
    enum Task<'a> {
        Visit(&'a FinalDocObjFix<'a>),
        Comp(bool),
    }
    let mut tasks: Vec<Task> = vec![Task::Visit(fix)];
    let mut out: Vec<Box<DocObjFix>> = Vec::new();
    while let Some(task) = tasks.pop() {
        match task {
            Task::Visit(f) => match f {
                FinalDocObjFix::Text(data) => out.push(Box::new(DocObjFix::Text(data.to_string()))),
                FinalDocObjFix::Comp(left, right, pad) => {
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
    out.pop().expect("fix conversion produced no result")
}

/// Rebuild an object bottom-up (iterative).
fn _convert_obj(obj: &FinalDocObj) -> Box<DocObj> {
    enum Task<'a> {
        Visit(&'a FinalDocObj<'a>),
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
                FinalDocObj::Text(data) => out.push(Box::new(DocObj::Text(data.to_string()))),
                // Fixed subtrees are self-contained and rebuilt by their own
                // iterative pass.
                FinalDocObj::Fix(fix) => out.push(Box::new(DocObj::Fix(_convert_fix(fix)))),
                FinalDocObj::Grp(obj1) => {
                    tasks.push(Task::Grp);
                    tasks.push(Task::Visit(obj1));
                }
                FinalDocObj::Seq(obj1) => {
                    tasks.push(Task::Seq);
                    tasks.push(Task::Visit(obj1));
                }
                FinalDocObj::Nest(obj1) => {
                    tasks.push(Task::Nest);
                    tasks.push(Task::Visit(obj1));
                }
                FinalDocObj::Pack(index, obj1) => {
                    tasks.push(Task::Pack(*index));
                    tasks.push(Task::Visit(obj1));
                }
                FinalDocObj::Comp(left, right, pad) => {
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
    out.pop().expect("obj conversion produced no result")
}

/// Move document from bump allocator to heap
pub fn move_to_heap<'a>(doc: &'a FinalDoc<'a>) -> Box<Doc> {
    // Walk the spine into a Vec; it terminates at `Eod` or `Line`.
    let mut spine: Vec<&FinalDoc> = Vec::new();
    let mut node = doc;
    loop {
        spine.push(node);
        match node {
            FinalDoc::Eod | FinalDoc::Line(_) => break,
            FinalDoc::Empty(doc1) | FinalDoc::Break(_, doc1) => node = doc1,
        }
    }
    // Fold from the tail so each spine node wraps the already-built remainder.
    let mut acc: Option<Box<Doc>> = None;
    for node in spine.into_iter().rev() {
        let built = match node {
            FinalDoc::Eod => Box::new(Doc::Eod),
            FinalDoc::Line(obj) => Box::new(Doc::Line(_convert_obj(obj))),
            FinalDoc::Empty(_) => Box::new(Doc::Empty(acc.take().expect("empty: tail"))),
            FinalDoc::Break(obj, _) => Box::new(Doc::Break(
                _convert_obj(obj),
                acc.take().expect("break: tail"),
            )),
        };
        acc = Some(built);
    }
    acc.expect("empty spine")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bumpalo::Bump;

    // Past where a recursive conversion aborted.
    const DEEP: usize = 50_000;

    #[test]
    fn deep_nested_obj_converts_without_overflow() {
        let mem = Bump::new();
        let mut obj: &FinalDocObj = mem.alloc(FinalDocObj::Text("x"));
        for _ in 0..DEEP {
            obj = mem.alloc(FinalDocObj::Nest(obj));
        }
        let doc: &FinalDoc = mem.alloc(FinalDoc::Line(obj));
        let heap = move_to_heap(doc);
        match &*heap {
            Doc::Line(_) => {}
            _ => panic!("expected Line"),
        }
        // `heap` drops here via the iterative Drop.
    }

    #[test]
    fn deep_comp_obj_converts_without_overflow() {
        let mem = Bump::new();
        let mut obj: &FinalDocObj = mem.alloc(FinalDocObj::Text("x"));
        for _ in 0..DEEP {
            let right: &FinalDocObj = mem.alloc(FinalDocObj::Text("y"));
            obj = mem.alloc(FinalDocObj::Comp(obj, right, false));
        }
        let doc: &FinalDoc = mem.alloc(FinalDoc::Line(obj));
        let _heap = move_to_heap(doc);
    }

    #[test]
    fn deep_spine_converts_without_overflow() {
        let mem = Bump::new();
        let mut node: &FinalDoc = mem.alloc(FinalDoc::Eod);
        for _ in 0..DEEP {
            node = mem.alloc(FinalDoc::Empty(node));
        }
        let heap = move_to_heap(node);
        // Count the spine to confirm the whole chain converted.
        let mut count = 0usize;
        let mut cur: &Doc = &heap;
        loop {
            match cur {
                Doc::Empty(d) => {
                    count += 1;
                    cur = d;
                }
                Doc::Eod => break,
                _ => panic!("unexpected spine node"),
            }
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn simple_conversion_shape() {
        let mem = Bump::new();
        let a: &FinalDocObj = mem.alloc(FinalDocObj::Text("a"));
        let b: &FinalDocObj = mem.alloc(FinalDocObj::Text("b"));
        let comp: &FinalDocObj = mem.alloc(FinalDocObj::Comp(a, b, true));
        let grp: &FinalDocObj = mem.alloc(FinalDocObj::Grp(comp));
        let doc: &FinalDoc = mem.alloc(FinalDoc::Line(grp));
        let heap = move_to_heap(doc);
        let rendered = format!("{}", *heap);
        assert_eq!(rendered, "Line (Grp (Comp (Text \"a\") (Text \"b\") true))");
    }
}
