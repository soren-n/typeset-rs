use std::fmt;

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

impl fmt::Display for Doc {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fn _print_doc(doc: Box<Doc>) -> String {
            match doc {
                box Doc::Eod => "Eod".to_string(),
                box Doc::Empty(doc1) => {
                    let doc_s = _print_doc(doc1);
                    format!("Empty\n{}", doc_s)
                }
                box Doc::Break(obj, doc1) => {
                    let obj_s = _print_obj(*obj);
                    let doc1_s = _print_doc(doc1);
                    format!("Break {}\n{}", obj_s, doc1_s)
                }
                box Doc::Line(obj) => {
                    let obj_s = _print_obj(*obj);
                    format!("Line {}", obj_s)
                }
            }
        }
        fn _print_obj(obj: DocObj) -> String {
            match obj {
                DocObj::Text(data) => format!("(Text \"{}\")", data),
                DocObj::Fix(obj1) => {
                    let obj_s = _print_fix(*obj1);
                    format!("(Fix {})", obj_s)
                }
                DocObj::Grp(obj1) => {
                    let obj_s = _print_obj(*obj1);
                    format!("(Grp {})", obj_s)
                }
                DocObj::Seq(obj1) => {
                    let obj_s = _print_obj(*obj1);
                    format!("(Seq {})", obj_s)
                }
                DocObj::Nest(obj1) => {
                    let obj_s = _print_obj(*obj1);
                    format!("(Nest {})", obj_s)
                }
                DocObj::Pack(index, obj1) => {
                    let obj_s = _print_obj(*obj1);
                    format!("(Pack {} {})", index, obj_s)
                }
                DocObj::Comp(left, right, pad) => {
                    let left_s = _print_obj(*left);
                    let right_s = _print_obj(*right);
                    format!("(Comp {} {} {})", left_s, right_s, pad)
                }
            }
        }
        fn _print_fix(obj: DocObjFix) -> String {
            match obj {
                DocObjFix::Text(data) => format!("(Text \"{}\")", data),
                DocObjFix::Comp(left, right, pad) => {
                    let left_s = _print_fix(*left);
                    let right_s = _print_fix(*right);
                    format!("(Comp {} {} {})", left_s, right_s, pad)
                }
            }
        }
        write!(f, "{}", _print_doc(Box::new(self.clone())))
    }
}
