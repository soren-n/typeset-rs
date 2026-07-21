use std::fmt;
use std::mem;

/// Final document representation - output of the compiler
#[derive(Debug, Clone)]
pub enum Doc {
    Eod,
    Empty(Box<Doc>),
    Break(Box<DocObj>, Box<Doc>),
    Line(Box<DocObj>),
}

#[derive(Debug, Clone)]
pub enum DocObj {
    Text(String),
    Fix(Box<DocObjFix>),
    Grp(Box<DocObj>),
    Seq(Box<DocObj>),
    Nest(Box<DocObj>),
    Pack(u64, Box<DocObj>),
    Comp(Box<DocObj>, Box<DocObj>, bool),
}

#[derive(Debug, Clone)]
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

// Each `_dismantle_*` moves the node's same-typed children onto the worklist
// (moving them out of their `Box`, freeing it, and leaving a leaf placeholder in
// the parent) and leaves any cross-type child in place to be freed by that
// type's own iterative `Drop`.

fn _dismantle_doc(node: &mut Doc, stack: &mut Vec<Doc>) {
    match node {
        Doc::Eod | Doc::Line(_) => {}
        Doc::Empty(doc1) | Doc::Break(_, doc1) => {
            stack.push(*mem::replace(doc1, Box::new(Doc::Eod)));
        }
    }
}

impl Drop for Doc {
    fn drop(&mut self) {
        let mut stack: Vec<Doc> = Vec::new();
        _dismantle_doc(self, &mut stack);
        while let Some(mut node) = stack.pop() {
            _dismantle_doc(&mut node, &mut stack);
        }
    }
}

fn _dismantle_obj(node: &mut DocObj, stack: &mut Vec<DocObj>) {
    match node {
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

impl Drop for DocObj {
    fn drop(&mut self) {
        let mut stack: Vec<DocObj> = Vec::new();
        _dismantle_obj(self, &mut stack);
        while let Some(mut node) = stack.pop() {
            _dismantle_obj(&mut node, &mut stack);
        }
    }
}

fn _dismantle_fix(node: &mut DocObjFix, stack: &mut Vec<DocObjFix>) {
    match node {
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

impl Drop for DocObjFix {
    fn drop(&mut self) {
        let mut stack: Vec<DocObjFix> = Vec::new();
        _dismantle_fix(self, &mut stack);
        while let Some(mut node) = stack.pop() {
            _dismantle_fix(&mut node, &mut stack);
        }
    }
}

/// Serialize a document object to the parenthesized debug form, iteratively.
///
/// Borrows the object (a `Drop`-carrying type cannot be moved out of) and walks
/// it with an explicit task stack so arbitrarily deep objects print without
/// recursing on the native stack.
fn _print_obj(obj: &DocObj) -> String {
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
        // objects print via the iterative `_print_obj` above.
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
                    out.push_str(&_print_obj(obj));
                    out.push('\n');
                    node = doc1;
                }
                Doc::Line(obj) => {
                    out.push_str("Line ");
                    out.push_str(&_print_obj(obj));
                    break;
                }
            }
        }
        write!(f, "{}", out)
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
}
