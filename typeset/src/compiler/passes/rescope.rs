//! rescope (final): DenullDoc → Doc (rescope nest and pack, into the heap)
//!
//! Each term arrives with its nest/pack wrappers already stripped into a prop
//! list (index 0 = outermost). This pass factors the common prefix shared by a
//! composition's two operands back out around the composition (rescoping),
//! applying the leftover props to each operand individually.
//!
//! This is the last pass, so it builds the owned [`Doc`] directly:
//! object/fixed-object nodes are pushed into a [`DocBuilder`] children-first,
//! so a parent's child ids always already exist. The input is a flat
//! postorder arena, so both walks are plain forward folds — children's results
//! are already computed when a parent is visited — and the spine is a row map.

use super::denull::DenullDoc;
use crate::compiler::types::{
    DenullTerm, Doc, DocBuilder, Fix, FixId, FixNode, IdVec, Obj, ObjId, ObjNode, Prop, Range,
};

/// Rescope nest and pack, lowering the flat `DenullDoc` into the `Doc`.
pub fn rescope(doc: DenullDoc) -> Doc {
    let DenullDoc {
        lines,
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
    let mut fix_res: IdVec<Fix<DenullTerm>, (Range<Prop>, FixId)> =
        IdVec::with_capacity(fixes.len());
    for (_, node) in fixes.iter() {
        let val = match *node {
            Fix::Term(term) => {
                let span = b.text(term.text);
                (term.props, b.fix(FixNode::Text(span)))
            }
            Fix::Comp(left, right, pad) => {
                let (l_props, left1) = fix_res[left];
                let right1 = fix_res[right].1;
                (l_props, b.fix(FixNode::Comp(left1, right1, pad)))
            }
        };
        fix_res.push(val);
    }

    // Fold the object arena bottom-up the same way; compositions factor the
    // common prop prefix out around themselves.
    let mut obj_res: IdVec<Obj<DenullTerm>, (Range<Prop>, ObjId)> =
        IdVec::with_capacity(objs.len());
    for (_, node) in objs.iter() {
        let val = match *node {
            Obj::Term(term) => {
                let span = b.text(term.text);
                (term.props, b.obj(ObjNode::Text(span)))
            }
            Obj::Fix(fix) => {
                let (fix_props, fix1) = fix_res[fix];
                (fix_props, b.obj(ObjNode::Fix(fix1)))
            }
            Obj::Grp(obj1) => {
                let (obj_props, id1) = obj_res[obj1];
                (obj_props, b.obj(ObjNode::Grp(id1)))
            }
            Obj::Seq(obj1) => {
                let (obj_props, id1) = obj_res[obj1];
                (obj_props, b.obj(ObjNode::Seq(id1)))
            }
            Obj::Comp(left, right, pad) => {
                let (l_props, left1) = obj_res[left];
                let (r_props, right1) = obj_res[right];
                // Factor the common prop prefix out around the composition;
                // apply the leftovers to each operand individually. Prefix and
                // leftovers are subranges of the operands' ranges.
                let l = l_props.slice(&props);
                let r = r_props.slice(&props);
                let k = common_prefix_len(l, r);
                let left2 = wrap_props(&mut b, &l[k..], left1);
                let right2 = wrap_props(&mut b, &r[k..], right1);
                let comp = b.obj(ObjNode::Comp(left2, right2, pad));
                let prefix = Range::new(l_props.start(), l_props.start() + k);
                (prefix, comp)
            }
        };
        obj_res.push(val);
    }

    // Map each line's root, re-applying its remaining prop prefix.
    let lines = lines
        .into_iter()
        .map(|root| {
            root.map(|id| {
                let (root_props, root) = obj_res[id];
                wrap_props(&mut b, root_props.slice(&props), root)
            })
        })
        .collect();

    b.finish(lines)
}

/// Length of the common prop prefix of `l` and `r`.
fn common_prefix_len(l: &[Prop], r: &[Prop]) -> usize {
    l.iter().zip(r.iter()).take_while(|(a, b)| a == b).count()
}

/// Wraps an object with its props (index 0 outermost), returning the id of the
/// outermost wrapper.
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
    use super::super::denull::DObjId;
    use super::*;
    use crate::compiler::types::{Arena, Pad};

    const DEEP: usize = 50_000;

    /// Pushes `props` onto the shared buffer and returns a term over them.
    fn prop_term(
        buf: &mut Vec<Prop>,
        props: impl IntoIterator<Item = Prop>,
        text: &'static str,
    ) -> Obj<DenullTerm<'static>> {
        let start = buf.len();
        buf.extend(props);
        Obj::Term(DenullTerm {
            props: Range::new(start, buf.len()),
            text,
        })
    }

    fn nest_term(
        buf: &mut Vec<Prop>,
        depth: usize,
        text: &'static str,
    ) -> Obj<DenullTerm<'static>> {
        prop_term(buf, std::iter::repeat_n(Prop::Nest, depth), text)
    }

    fn line_doc<'a>(
        objs: Arena<Obj<DenullTerm<'a>>>,
        props: Vec<Prop>,
        root: DObjId<'a>,
    ) -> DenullDoc<'a> {
        DenullDoc {
            lines: vec![Some(root)],
            objs,
            fixes: Arena::new(),
            props,
        }
    }

    /// The single object id a one-line document holds.
    fn line_root(doc: &Doc) -> ObjId {
        match doc.lines[..] {
            [Some(id)] => id,
            _ => panic!("expected a single-line document"),
        }
    }

    #[test]
    fn rescope_reapplies_deep_nest_props() {
        let mut buf: Vec<Prop> = Vec::new();
        let mut objs: Arena<Obj<DenullTerm>> = Arena::new();
        let root = objs.push(nest_term(&mut buf, DEEP, "x"));
        let out = rescope(line_doc(objs, buf, root));
        // The stripped nests are re-applied around the text.
        let mut count = 0usize;
        let mut cur = line_root(&out);
        while let ObjNode::Nest(inner) = out.objs[cur] {
            count += 1;
            cur = inner;
        }
        assert!(matches!(out.objs[cur], ObjNode::Text(_)));
        assert_eq!(count, DEEP);
    }

    #[test]
    fn rescope_factors_deep_shared_nest_prefix() {
        // Both operands share a Nest^DEEP prefix: rescoping lifts all of it
        // out around the composition.
        let mut buf: Vec<Prop> = Vec::new();
        let mut objs: Arena<Obj<DenullTerm>> = Arena::new();
        let a = objs.push(nest_term(&mut buf, DEEP, "a"));
        let bx = objs.push(nest_term(&mut buf, DEEP, "b"));
        let root = objs.push(Obj::Comp(a, bx, Pad::Unpadded));
        let out = rescope(line_doc(objs, buf, root));
        let mut count = 0usize;
        let mut cur = line_root(&out);
        while let ObjNode::Nest(inner) = out.objs[cur] {
            count += 1;
            cur = inner;
        }
        // The common nests wrap a single composition of the bare texts.
        assert_eq!(count, DEEP);
        let ObjNode::Comp(left, right, _) = out.objs[cur] else {
            panic!("expected the lifted comp")
        };
        assert!(matches!(out.objs[left], ObjNode::Text(_)));
        assert!(matches!(out.objs[right], ObjNode::Text(_)));
    }

    #[test]
    fn rescope_splits_diverging_prop_prefixes() {
        // Left is Nest(text), right is Pack(text): no common prefix, so each
        // operand keeps its own wrapper under the composition.
        let mut buf: Vec<Prop> = Vec::new();
        let mut objs: Arena<Obj<DenullTerm>> = Arena::new();
        let a = objs.push(prop_term(&mut buf, [Prop::Nest], "a"));
        let bx = objs.push(prop_term(&mut buf, [Prop::Pack(3)], "b"));
        let root = objs.push(Obj::Comp(a, bx, Pad::Padded));
        let out = rescope(line_doc(objs, buf, root));
        let ObjNode::Comp(left, right, pad) = out.objs[line_root(&out)] else {
            panic!("expected a comp root")
        };
        assert_eq!(pad, Pad::Padded);
        assert!(matches!(out.objs[left], ObjNode::Nest(_)));
        assert!(matches!(out.objs[right], ObjNode::Pack(3, _)));
    }

    #[test]
    fn rescope_fix_comp_keeps_left_props_only() {
        let mut objs: Arena<Obj<DenullTerm>> = Arena::new();
        let mut fixes: Arena<Fix<DenullTerm>> = Arena::new();
        // Both fix terms carry one Nest; their ranges share the buffer.
        let buf = vec![Prop::Nest, Prop::Nest];
        let fa = fixes.push(Fix::Term(DenullTerm {
            props: Range::new(0, 1),
            text: "a",
        }));
        let fb = fixes.push(Fix::Term(DenullTerm {
            props: Range::new(1, 2),
            text: "b",
        }));
        let fc = fixes.push(Fix::Comp(fa, fb, Pad::Unpadded));
        let root = objs.push(Obj::Fix(fc));
        let doc = DenullDoc {
            lines: vec![Some(root)],
            objs,
            fixes,
            props: buf,
        };
        let out = rescope(doc);
        // The left operand's nest surfaces around the fix; the right's is
        // dropped inside it.
        let mut cur = line_root(&out);
        let ObjNode::Nest(inner) = out.objs[cur] else {
            panic!("expected the left props around the fix")
        };
        cur = inner;
        assert!(matches!(out.objs[cur], ObjNode::Fix(_)));
    }
}
