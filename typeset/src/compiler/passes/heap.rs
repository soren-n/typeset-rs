//! Pass 10: FinalDoc → Doc (move to heap)

use crate::compiler::types::{Doc, DocObj, DocObjFix, FinalDoc, FinalDocObj, FinalDocObjFix};

/// Move document from bump allocator to heap
pub fn move_to_heap<'a>(doc: &'a FinalDoc<'a>) -> Box<Doc> {
    fn _visit_doc<'a>(doc: &'a FinalDoc<'a>) -> Box<Doc> {
        match doc {
            FinalDoc::Eod => Box::new(Doc::Eod),
            FinalDoc::Empty(doc1) => {
                let doc2 = _visit_doc(doc1);
                Box::new(Doc::Empty(doc2))
            }
            FinalDoc::Break(obj, doc1) => {
                let obj1 = _visit_obj(obj);
                let doc2 = _visit_doc(doc1);
                Box::new(Doc::Break(obj1, doc2))
            }
            FinalDoc::Line(obj) => {
                let obj1 = _visit_obj(obj);
                Box::new(Doc::Line(obj1))
            }
        }
    }
    fn _visit_obj<'a>(obj: &'a FinalDocObj<'a>) -> Box<DocObj> {
        match obj {
            FinalDocObj::Text(data) => Box::new(DocObj::Text(data.to_string())),
            FinalDocObj::Fix(fix) => {
                let fix1 = _visit_fix(fix);
                Box::new(DocObj::Fix(fix1))
            }
            FinalDocObj::Grp(obj1) => {
                let obj2 = _visit_obj(obj1);
                Box::new(DocObj::Grp(obj2))
            }
            FinalDocObj::Seq(obj1) => {
                let obj2 = _visit_obj(obj1);
                Box::new(DocObj::Seq(obj2))
            }
            FinalDocObj::Nest(obj1) => {
                let obj2 = _visit_obj(obj1);
                Box::new(DocObj::Nest(obj2))
            }
            FinalDocObj::Pack(index, obj1) => {
                let obj2 = _visit_obj(obj1);
                Box::new(DocObj::Pack(*index, obj2))
            }
            FinalDocObj::Comp(left, right, pad) => {
                let left1 = _visit_obj(left);
                let right1 = _visit_obj(right);
                Box::new(DocObj::Comp(left1, right1, *pad))
            }
        }
    }
    fn _visit_fix<'a>(fix: &'a FinalDocObjFix<'a>) -> Box<DocObjFix> {
        match fix {
            FinalDocObjFix::Text(data) => Box::new(DocObjFix::Text(data.to_string())),
            FinalDocObjFix::Comp(left, right, pad) => {
                let left1 = _visit_fix(left);
                let right1 = _visit_fix(right);
                Box::new(DocObjFix::Comp(left1, right1, *pad))
            }
        }
    }
    _visit_doc(doc)
}
