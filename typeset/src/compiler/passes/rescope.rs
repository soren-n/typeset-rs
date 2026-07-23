//! rescope (final): DenullDoc → Doc (rescope nest and pack, into the heap)
//!
//! Each term arrives with its nest/pack wrappers already stripped into a prop
//! list (index 0 = outermost). This pass factors the common prefix shared by a
//! composition's two operands back out around the composition (rescoping),
//! applying the leftover props to each operand individually.
//!
//! This is the last pass, so it builds the owned heap [`Doc`] directly:
//! object/fixed-object nodes are pushed into a [`DocBuilder`] children-first,
//! so a parent's child indices always already exist. The input is a flat
//! postorder arena, so both walks are plain forward folds — children's results
//! are already computed when a parent is visited — and the spine is a row map.

use crate::compiler::types::{
    DenullDoc, DenullFix, DenullObj, DenullRow, Doc, DocBuilder, FixId, FixNode, ObjId, ObjNode,
    Prop, Row,
};

/// Rescope nest and pack, lowering the flat `DenullDoc` into the heap `Doc`.
pub fn rescope(doc: DenullDoc) -> Box<Doc> {
    // The output holds at least one node per input node (plus re-applied
    // props), and exactly one fixed node per input fixed node.
    let mut b = DocBuilder::with_capacity(doc.objs.len(), doc.fixes.len());

    // Fold the fixed-object arena bottom-up. A fix composition keeps only its
    // left operand's props (the right operand's are dropped).
    let mut fix_res: Vec<(Vec<Prop>, FixId)> = Vec::with_capacity(doc.fixes.len());
    for node in doc.fixes {
        let val = match node {
            DenullFix::Term(term) => {
                let span = b.text(term.text);
                (term.props, b.fix(FixNode::Text(span)))
            }
            DenullFix::Comp(left, right, pad) => {
                let l_props = std::mem::take(&mut fix_res[left as usize].0);
                let left1 = fix_res[left as usize].1;
                let right1 = fix_res[right as usize].1;
                (l_props, b.fix(FixNode::Comp(left1, right1, pad)))
            }
        };
        fix_res.push(val);
    }

    // Fold the object arena bottom-up the same way; compositions factor the
    // common prop prefix out around themselves.
    let mut obj_res: Vec<(Vec<Prop>, ObjId)> = Vec::with_capacity(doc.objs.len());
    for node in doc.objs {
        let val = match node {
            DenullObj::Term(term) => {
                let span = b.text(term.text);
                (term.props, b.obj(ObjNode::Text(span)))
            }
            DenullObj::Fix(fix) => {
                let props = std::mem::take(&mut fix_res[fix as usize].0);
                (props, b.obj(ObjNode::Fix(fix_res[fix as usize].1)))
            }
            DenullObj::Grp(obj1) => {
                let props = std::mem::take(&mut obj_res[obj1 as usize].0);
                (props, b.obj(ObjNode::Grp(obj_res[obj1 as usize].1)))
            }
            DenullObj::Seq(obj1) => {
                let props = std::mem::take(&mut obj_res[obj1 as usize].0);
                (props, b.obj(ObjNode::Seq(obj_res[obj1 as usize].1)))
            }
            DenullObj::Comp(left, right, pad) => {
                let mut l_props = std::mem::take(&mut obj_res[left as usize].0);
                let r_props = std::mem::take(&mut obj_res[right as usize].0);
                // Factor the common prop prefix out around the composition;
                // apply the leftovers to each operand individually. `l_props`
                // is reused as the common prefix (truncated in place).
                let k = common_prefix_len(&l_props, &r_props);
                let left2 = wrap_props(&mut b, &l_props[k..], obj_res[left as usize].1);
                let right2 = wrap_props(&mut b, &r_props[k..], obj_res[right as usize].1);
                let comp = b.obj(ObjNode::Comp(left2, right2, pad));
                l_props.truncate(k);
                (l_props, comp)
            }
        };
        obj_res.push(val);
    }

    // Map the spine rows, re-applying each root's remaining prop prefix.
    let rows: Vec<Row> = doc
        .rows
        .into_iter()
        .map(|row| {
            let mut finish = |id: u32| {
                let props = std::mem::take(&mut obj_res[id as usize].0);
                wrap_props(&mut b, &props, obj_res[id as usize].1)
            };
            match row {
                DenullRow::Empty => Row::Empty,
                DenullRow::Break(id) => Row::Break(finish(id)),
                DenullRow::Line(id) => Row::Line(finish(id)),
            }
        })
        .collect();

    Box::new(b.finish(rows))
}

/// Length of the common prop prefix of `l` and `r`.
fn common_prefix_len(l: &[Prop], r: &[Prop]) -> usize {
    l.iter().zip(r.iter()).take_while(|(a, b)| a == b).count()
}

/// Wraps an object with its props (index 0 outermost), returning the arena index
/// of the outermost wrapper.
fn wrap_props(b: &mut DocBuilder, props: &[Prop], term: ObjId) -> ObjId {
    // Apply from the tail so the first prop ends up outermost.
    let mut obj = term;
    for prop in props.iter().rev() {
        obj = match prop {
            Prop::Nest => b.obj(ObjNode::Nest(obj)),
            Prop::Pack(index) => b.obj(ObjNode::Pack(*index, obj)),
        };
    }
    obj
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::{DObjId, DenullTerm, push_node};

    /// Far past where a native-stack recursion could survive; with flat arenas
    /// the folds are plain loops, so this now guards sizing behavior only.
    const DEEP: usize = 50_000;

    fn nest_term(depth: usize, text: &'static str) -> DenullObj<'static> {
        DenullObj::Term(DenullTerm {
            props: vec![Prop::Nest; depth],
            text,
        })
    }

    fn line_doc(objs: Vec<DenullObj>, root: DObjId) -> DenullDoc {
        DenullDoc {
            rows: vec![DenullRow::Line(root)],
            objs,
            fixes: Vec::new(),
        }
    }

    /// The single object index a one-line document holds.
    fn line_root(doc: &Doc) -> ObjId {
        match doc.rows() {
            [Row::Line(id)] => *id,
            _ => panic!("expected a single-line document"),
        }
    }

    #[test]
    fn rescope_reapplies_deep_nest_props() {
        let mut objs: Vec<DenullObj> = Vec::new();
        let root = push_node(&mut objs, nest_term(DEEP, "x"));
        let out = rescope(line_doc(objs, root));
        // The stripped nests are re-applied around the text.
        let mut count = 0usize;
        let mut cur = line_root(&out);
        while let ObjNode::Nest(inner) = out.objs()[cur as usize] {
            count += 1;
            cur = inner;
        }
        assert!(matches!(out.objs()[cur as usize], ObjNode::Text(_)));
        assert_eq!(count, DEEP);
    }

    #[test]
    fn rescope_factors_deep_shared_nest_prefix() {
        // Both operands share a Nest^DEEP prefix: rescoping lifts all of it
        // out around the composition.
        let mut objs: Vec<DenullObj> = Vec::new();
        let a = push_node(&mut objs, nest_term(DEEP, "a"));
        let bx = push_node(&mut objs, nest_term(DEEP, "b"));
        let root = push_node(&mut objs, DenullObj::Comp(a, bx, false));
        let out = rescope(line_doc(objs, root));
        let mut count = 0usize;
        let mut cur = line_root(&out);
        while let ObjNode::Nest(inner) = out.objs()[cur as usize] {
            count += 1;
            cur = inner;
        }
        // The common nests wrap a single composition of the bare texts.
        assert_eq!(count, DEEP);
        let ObjNode::Comp(left, right, _) = out.objs()[cur as usize] else {
            panic!("expected the lifted comp")
        };
        assert!(matches!(out.objs()[left as usize], ObjNode::Text(_)));
        assert!(matches!(out.objs()[right as usize], ObjNode::Text(_)));
    }

    #[test]
    fn rescope_splits_diverging_prop_prefixes() {
        // Left is Nest(text), right is Pack(text): no common prefix, so each
        // operand keeps its own wrapper under the composition.
        let mut objs: Vec<DenullObj> = Vec::new();
        let a = push_node(
            &mut objs,
            DenullObj::Term(DenullTerm {
                props: vec![Prop::Nest],
                text: "a",
            }),
        );
        let bx = push_node(
            &mut objs,
            DenullObj::Term(DenullTerm {
                props: vec![Prop::Pack(3)],
                text: "b",
            }),
        );
        let root = push_node(&mut objs, DenullObj::Comp(a, bx, true));
        let out = rescope(line_doc(objs, root));
        let ObjNode::Comp(left, right, pad) = out.objs()[line_root(&out) as usize] else {
            panic!("expected a comp root")
        };
        assert!(pad);
        assert!(matches!(out.objs()[left as usize], ObjNode::Nest(_)));
        assert!(matches!(out.objs()[right as usize], ObjNode::Pack(3, _)));
    }

    #[test]
    fn rescope_handles_deep_comp_object() {
        // Right-nested comp of plain terms.
        let mut objs: Vec<DenullObj> = Vec::new();
        let mut cur = push_node(&mut objs, nest_term(0, "z"));
        for _ in 0..DEEP {
            let left = push_node(&mut objs, nest_term(0, "y"));
            cur = push_node(&mut objs, DenullObj::Comp(left, cur, false));
        }
        let out = rescope(line_doc(objs, cur));
        let mut count = 0usize;
        let mut walk = line_root(&out);
        while let ObjNode::Comp(_left, right, _) = out.objs()[walk as usize] {
            count += 1;
            walk = right;
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn rescope_fix_comp_keeps_left_props_only() {
        let mut objs: Vec<DenullObj> = Vec::new();
        let mut fixes: Vec<DenullFix> = Vec::new();
        let fa = push_node(
            &mut fixes,
            DenullFix::Term(DenullTerm {
                props: vec![Prop::Nest],
                text: "a",
            }),
        );
        let fb = push_node(
            &mut fixes,
            DenullFix::Term(DenullTerm {
                props: vec![Prop::Nest],
                text: "b",
            }),
        );
        let fc = push_node(&mut fixes, DenullFix::Comp(fa, fb, false));
        let root = push_node(&mut objs, DenullObj::Fix(fc));
        let doc = DenullDoc {
            rows: vec![DenullRow::Line(root)],
            objs,
            fixes,
        };
        let out = rescope(doc);
        // The left operand's nest surfaces around the fix; the right's is
        // dropped inside it.
        let mut cur = line_root(&out);
        let ObjNode::Nest(inner) = out.objs()[cur as usize] else {
            panic!("expected the left props around the fix")
        };
        cur = inner;
        assert!(matches!(out.objs()[cur as usize], ObjNode::Fix(_)));
    }

    #[test]
    fn rescope_handles_long_doc_spine() {
        let mut objs: Vec<DenullObj> = Vec::new();
        let mut rows: Vec<DenullRow> = Vec::new();
        for _ in 0..DEEP {
            rows.push(DenullRow::Break(push_node(&mut objs, nest_term(0, "x"))));
        }
        let doc = DenullDoc {
            rows,
            objs,
            fixes: Vec::new(),
        };
        let out = rescope(doc);
        // Eod-terminated spine: DEEP Break rows and no Line row.
        let count = out
            .rows()
            .iter()
            .filter(|r| matches!(r, Row::Break(_)))
            .count();
        assert!(!out.rows().iter().any(|r| matches!(r, Row::Line(_))));
        assert_eq!(count, DEEP);
    }
}
