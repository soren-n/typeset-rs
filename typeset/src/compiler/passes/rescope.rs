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
    Prop, Props, Row,
};

/// Rescope nest and pack, lowering the flat `DenullDoc` into the heap `Doc`.
pub fn rescope(doc: DenullDoc) -> Box<Doc> {
    let DenullDoc {
        rows,
        objs,
        fixes,
        props,
    } = doc;

    // The output holds at least one node per input node (plus re-applied
    // props), and exactly one fixed node per input fixed node.
    let mut b = DocBuilder::with_capacity(objs.len(), fixes.len());

    // Fold the fixed-object arena bottom-up. A fix composition keeps only its
    // left operand's props (the right operand's are dropped). Prop lists are
    // ranges into the shared buffer, so results are plain copyable pairs.
    let mut fix_res: Vec<(Props, FixId)> = Vec::with_capacity(fixes.len());
    for node in fixes {
        let val = match node {
            DenullFix::Term(term) => {
                let span = b.text(term.text);
                (term.props, b.fix(FixNode::Text(span)))
            }
            DenullFix::Comp(left, right, pad) => {
                let (l_props, left1) = fix_res[left as usize];
                let right1 = fix_res[right as usize].1;
                (l_props, b.fix(FixNode::Comp(left1, right1, pad)))
            }
        };
        fix_res.push(val);
    }

    // Fold the object arena bottom-up the same way; compositions factor the
    // common prop prefix out around themselves.
    let mut obj_res: Vec<(Props, ObjId)> = Vec::with_capacity(objs.len());
    for node in objs {
        let val = match node {
            DenullObj::Term(term) => {
                let span = b.text(term.text);
                (term.props, b.obj(ObjNode::Text(span)))
            }
            DenullObj::Fix(fix) => {
                let (fix_props, fix1) = fix_res[fix as usize];
                (fix_props, b.obj(ObjNode::Fix(fix1)))
            }
            DenullObj::Grp(obj1) => {
                let (obj_props, id1) = obj_res[obj1 as usize];
                (obj_props, b.obj(ObjNode::Grp(id1)))
            }
            DenullObj::Seq(obj1) => {
                let (obj_props, id1) = obj_res[obj1 as usize];
                (obj_props, b.obj(ObjNode::Seq(id1)))
            }
            DenullObj::Comp(left, right, pad) => {
                let (l_props, left1) = obj_res[left as usize];
                let (r_props, right1) = obj_res[right as usize];
                // Factor the common prop prefix out around the composition;
                // apply the leftovers to each operand individually. Prefix and
                // leftovers are subranges of the operands' ranges.
                let l = l_props.slice(&props);
                let r = r_props.slice(&props);
                let k = common_prefix_len(l, r);
                let left2 = wrap_props(&mut b, &l[k..], left1);
                let right2 = wrap_props(&mut b, &r[k..], right1);
                let comp = b.obj(ObjNode::Comp(left2, right2, pad));
                let prefix = Props {
                    start: l_props.start,
                    end: l_props.start + k as u32,
                };
                (prefix, comp)
            }
        };
        obj_res.push(val);
    }

    // Map the spine rows, re-applying each root's remaining prop prefix.
    let rows: Vec<Row> = rows
        .into_iter()
        .map(|row| {
            let mut finish = |id: u32| {
                let (root_props, root) = obj_res[id as usize];
                wrap_props(&mut b, root_props.slice(&props), root)
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

    /// Pushes `props` onto the shared buffer and returns a term over them.
    fn prop_term(
        buf: &mut Vec<Prop>,
        props: impl IntoIterator<Item = Prop>,
        text: &'static str,
    ) -> DenullObj<'static> {
        let start = buf.len() as u32;
        buf.extend(props);
        DenullObj::Term(DenullTerm {
            props: Props {
                start,
                end: buf.len() as u32,
            },
            text,
        })
    }

    fn nest_term(buf: &mut Vec<Prop>, depth: usize, text: &'static str) -> DenullObj<'static> {
        prop_term(buf, std::iter::repeat_n(Prop::Nest, depth), text)
    }

    fn line_doc(objs: Vec<DenullObj>, props: Vec<Prop>, root: DObjId) -> DenullDoc {
        DenullDoc {
            rows: vec![DenullRow::Line(root)],
            objs,
            fixes: Vec::new(),
            props,
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
        let mut buf: Vec<Prop> = Vec::new();
        let mut objs: Vec<DenullObj> = Vec::new();
        let root = push_node(&mut objs, nest_term(&mut buf, DEEP, "x"));
        let out = rescope(line_doc(objs, buf, root));
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
        let mut buf: Vec<Prop> = Vec::new();
        let mut objs: Vec<DenullObj> = Vec::new();
        let a = push_node(&mut objs, nest_term(&mut buf, DEEP, "a"));
        let bx = push_node(&mut objs, nest_term(&mut buf, DEEP, "b"));
        let root = push_node(&mut objs, DenullObj::Comp(a, bx, false));
        let out = rescope(line_doc(objs, buf, root));
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
        let mut buf: Vec<Prop> = Vec::new();
        let mut objs: Vec<DenullObj> = Vec::new();
        let a = push_node(&mut objs, prop_term(&mut buf, [Prop::Nest], "a"));
        let bx = push_node(&mut objs, prop_term(&mut buf, [Prop::Pack(3)], "b"));
        let root = push_node(&mut objs, DenullObj::Comp(a, bx, true));
        let out = rescope(line_doc(objs, buf, root));
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
        let mut buf: Vec<Prop> = Vec::new();
        let mut objs: Vec<DenullObj> = Vec::new();
        let mut cur = push_node(&mut objs, nest_term(&mut buf, 0, "z"));
        for _ in 0..DEEP {
            let left = push_node(&mut objs, nest_term(&mut buf, 0, "y"));
            cur = push_node(&mut objs, DenullObj::Comp(left, cur, false));
        }
        let out = rescope(line_doc(objs, buf, cur));
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
        // Both fix terms carry one Nest; their ranges share the buffer.
        let buf = vec![Prop::Nest, Prop::Nest];
        let fa = push_node(
            &mut fixes,
            DenullFix::Term(DenullTerm {
                props: Props { start: 0, end: 1 },
                text: "a",
            }),
        );
        let fb = push_node(
            &mut fixes,
            DenullFix::Term(DenullTerm {
                props: Props { start: 1, end: 2 },
                text: "b",
            }),
        );
        let fc = push_node(&mut fixes, DenullFix::Comp(fa, fb, false));
        let root = push_node(&mut objs, DenullObj::Fix(fc));
        let doc = DenullDoc {
            rows: vec![DenullRow::Line(root)],
            objs,
            fixes,
            props: buf,
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
        let mut buf: Vec<Prop> = Vec::new();
        let mut objs: Vec<DenullObj> = Vec::new();
        let mut rows: Vec<DenullRow> = Vec::new();
        for _ in 0..DEEP {
            rows.push(DenullRow::Break(push_node(
                &mut objs,
                nest_term(&mut buf, 0, "x"),
            )));
        }
        let doc = DenullDoc {
            rows,
            objs,
            fixes: Vec::new(),
            props: buf,
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
