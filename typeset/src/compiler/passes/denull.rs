//! denull: RebuildDoc → DenullDoc (remove null identities)
//!
//! Drops `Null`/empty-text terms, collapsing objects that reduce to nothing,
//! and strips each surviving term's nest/pack chain into a flat prop list.
//! Both input and output are flat postorder arenas (children precede parents),
//! so the object and fix walks are plain forward folds: by the time a node is
//! visited its children's results are already computed.

use super::resolve_scopes::RebuildDoc;
use crate::compiler::types::{
    Arena, DenullTerm, Fix, Id, IdVec, Obj, Pad, PathNode, Prop, Range, Term, TermLeaf,
};

pub(crate) type DObjId<'a> = Id<Obj<DenullTerm<'a>>>;
pub(crate) type DFixId<'a> = Id<Fix<DenullTerm<'a>>>;

/// The document with nulls gone: one optional root per line (`None` for a
/// line that denulled to nothing) over composition trees whose terms are a
/// stripped `(props, text)` pair. Both arenas are postorder.
#[derive(Debug)]
pub(crate) struct DenullDoc<'a> {
    pub(crate) lines: Vec<Option<DObjId<'a>>>,
    pub(crate) objs: Arena<Obj<DenullTerm<'a>>>,
    pub(crate) fixes: Arena<Fix<DenullTerm<'a>>>,
    /// The shared prop buffer every [`DenullTerm`]'s `props` range indexes.
    pub(crate) props: Vec<Prop>,
}

/// Result of denulling an object: nothing survived (`None`); an object
/// survived (`Some`); or everything to the left of a composition was dropped,
/// leaving a surviving object plus the accumulated pad (`NextNone`).
#[derive(Copy, Clone)]
enum Res<Id> {
    None,
    Some(Id),
    NextNone(Pad, Id),
}

/// Remove null identities.
pub fn denull<'a>(doc: &RebuildDoc<'a>, paths: &Arena<PathNode>) -> DenullDoc<'a> {
    // Every input node yields at most one output node, so the input sizes are
    // exact capacity bounds.
    let mut objs: Arena<Obj<DenullTerm<'a>>> = Arena::with_capacity(doc.objs.len());
    let mut fixes: Arena<Fix<DenullTerm<'a>>> = Arena::with_capacity(doc.fixes.len());
    // The shared prop buffer every surviving term's props range indexes, and
    // the memo of already-materialized paths: terms sharing a path (sibling
    // leaves under the same wrappers) share one materialization, so the
    // buffer is O(path arena), not O(terms × depth).
    let mut props: Vec<Prop> = Vec::new();
    let mut memo: IdVec<PathNode, Option<Range<Prop>>> = IdVec::filled(None, paths.len());

    // Fold the fixed-object arena bottom-up (forward, children first).
    let mut fix_res: IdVec<Fix<Term<'a>>, Res<DFixId<'a>>> = IdVec::with_capacity(doc.fixes.len());
    for (_, node) in doc.fixes.iter() {
        let res = match *node {
            Fix::Term(term) => match strip_term(&mut props, &mut memo, paths, term) {
                None => Res::None,
                Some(term1) => Res::Some(fixes.push(Fix::Term(term1))),
            },
            Fix::Comp(left, right, l_pad) => comp_res(
                fix_res[left],
                fix_res[right],
                l_pad,
                |left1, right1, pad| fixes.push(Fix::Comp(left1, right1, pad)),
            ),
        };
        fix_res.push(res);
    }

    // Fold the object arena bottom-up the same way.
    let mut obj_res: IdVec<Obj<Term<'a>>, Res<DObjId<'a>>> = IdVec::with_capacity(doc.objs.len());
    for (_, node) in doc.objs.iter() {
        let res = match *node {
            Obj::Term(term) => match strip_term(&mut props, &mut memo, paths, term) {
                None => Res::None,
                Some(term1) => Res::Some(objs.push(Obj::Term(term1))),
            },
            Obj::Fix(fix) => match fix_res[fix] {
                Res::None => Res::None,
                Res::Some(fix1) | Res::NextNone(_, fix1) => Res::Some(objs.push(Obj::Fix(fix1))),
            },
            Obj::Grp(obj1) => wrap_obj(&mut objs, obj_res[obj1], Obj::Grp),
            Obj::Seq(obj1) => wrap_obj(&mut objs, obj_res[obj1], Obj::Seq),
            Obj::Comp(left, right, l_pad) => comp_res(
                obj_res[left],
                obj_res[right],
                l_pad,
                |left1, right1, pad| objs.push(Obj::Comp(left1, right1, pad)),
            ),
        };
        obj_res.push(res);
    }

    // A line whose object survived keeps it; an emptied line is `None`.
    let lines = doc
        .lines
        .iter()
        .map(|&root| match obj_res[root] {
            Res::None => None,
            Res::Some(obj1) | Res::NextNone(_, obj1) => Some(obj1),
        })
        .collect();

    DenullDoc {
        lines,
        objs,
        fixes,
        props,
    }
}

/// The composition rule shared by the object and fix folds: a dropped left
/// operand forwards its pad (`NextNone`), a dropped right operand yields the
/// left alone, and two survivors compose (merging any forwarded pad).
fn comp_res<Id: Copy>(
    left: Res<Id>,
    right: Res<Id>,
    l_pad: Pad,
    mut comp: impl FnMut(Id, Id, Pad) -> Id,
) -> Res<Id> {
    match (left, right) {
        (Res::None, Res::None) => Res::None,
        (Res::None, Res::Some(right1)) => Res::NextNone(l_pad, right1),
        (Res::None, Res::NextNone(r_pad, right1)) => Res::NextNone(l_pad.merge(r_pad), right1),
        (Res::Some(left1), Res::None) => Res::Some(left1),
        (Res::Some(left1), Res::Some(right1)) => Res::Some(comp(left1, right1, l_pad)),
        (Res::Some(left1), Res::NextNone(r_pad, right1)) => {
            Res::Some(comp(left1, right1, l_pad.merge(r_pad)))
        }
        // A composition's left operand never denulls to NextNone.
        (Res::NextNone(..), _) => unreachable!("Invariant"),
    }
}

/// Wraps a denulled object in a `Grp` or `Seq` (whichever `ctor` builds),
/// propagating the "nothing survived" result unchanged. A surviving `NextNone`
/// collapses to `Some` — the dropped-left pad is discarded at a wrapper.
fn wrap_obj<'a>(
    objs: &mut Arena<Obj<DenullTerm<'a>>>,
    val: Res<DObjId<'a>>,
    ctor: fn(DObjId<'a>) -> Obj<DenullTerm<'a>>,
) -> Res<DObjId<'a>> {
    match val {
        Res::None => Res::None,
        Res::Some(obj) | Res::NextNone(_, obj) => Res::Some(objs.push(ctor(obj))),
    }
}

/// Denulls a term: `Null` and empty text vanish (wrappers and all); otherwise
/// the term's wrapper path is materialized outermost-first into the shared
/// prop buffer (memoized per path id — sibling terms under the same wrappers
/// share one materialization) and the term records the range.
fn strip_term<'a>(
    props: &mut Vec<Prop>,
    memo: &mut IdVec<PathNode, Option<Range<Prop>>>,
    paths: &Arena<PathNode>,
    term: Term<'a>,
) -> Option<DenullTerm<'a>> {
    let text = match term.leaf {
        TermLeaf::Null | TermLeaf::Text("") => return None,
        TermLeaf::Text(data) => data,
    };
    let Some(path) = term.path else {
        return Some(DenullTerm {
            props: Range::EMPTY,
            text,
        });
    };
    let range = match memo[path] {
        Some(range) => range,
        None => {
            let start = props.len();
            let mut cur = Some(path);
            while let Some(id) = cur {
                props.push(paths[id].prop);
                cur = paths[id].parent;
            }
            // The path walk yields innermost-first; prop lists are outermost-first.
            props[start..].reverse();
            let range = Range::new(start, props.len());
            memo[path] = Some(range);
            range
        }
    };
    Some(DenullTerm { props: range, text })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_term(text: &'static str) -> Term<'static> {
        Term {
            path: None,
            leaf: TermLeaf::Text(text),
        }
    }

    fn null_term() -> Term<'static> {
        Term {
            path: None,
            leaf: TermLeaf::Null,
        }
    }

    const DEEP: usize = 50_000;

    #[test]
    fn denull_strips_deep_nest_term_to_props() {
        let mut paths: Arena<PathNode> = Arena::new();
        let mut path = None;
        for _ in 0..DEEP {
            path = Some(paths.push(PathNode {
                prop: Prop::Nest,
                parent: path,
            }));
        }
        let term = Term {
            path,
            leaf: TermLeaf::Text("x"),
        };
        let mut objs: Arena<Obj<Term>> = Arena::new();
        let root = objs.push(Obj::Term(term));
        let doc = RebuildDoc {
            lines: vec![root],
            objs,
            fixes: Arena::new(),
        };
        let out = denull(&doc, &paths);
        let [Some(root)] = out.lines[..] else {
            panic!("expected a single line");
        };
        let Obj::Term(t) = &out.objs[root] else {
            panic!("expected a single term line");
        };
        assert_eq!(t.text, "x");
        let props = t.props.slice(&out.props);
        assert_eq!(props.len(), DEEP);
        assert!(props.iter().all(|p| matches!(p, Prop::Nest)));
    }

    #[test]
    fn denull_drops_null_left_and_merges_pads() {
        // Comp(Text "a", Comp(Null, Text "x", pad=true), pad=false): the null
        // vanishes and its pad merges onto the surviving composition.
        let mut objs: Arena<Obj<Term>> = Arena::new();
        let n1 = objs.push(Obj::Term(null_term()));
        let t = objs.push(Obj::Term(text_term("x")));
        let inner = objs.push(Obj::Comp(n1, t, Pad::Padded));
        let a = objs.push(Obj::Term(text_term("a")));
        let root = objs.push(Obj::Comp(a, inner, Pad::Unpadded));
        let doc = RebuildDoc {
            lines: vec![root],
            objs,
            fixes: Arena::new(),
        };
        let out = denull(&doc, &Arena::new());
        let [Some(root)] = out.lines[..] else {
            panic!("expected a single line");
        };
        let Obj::Comp(_, _, pad) = out.objs[root] else {
            panic!("expected a comp line");
        };
        assert_eq!(
            pad,
            Pad::Padded,
            "the dropped left's pad must merge into the comp"
        );
    }

    #[test]
    fn denull_emptied_lines_become_none() {
        // Lines: [Text "a", Null, Null]: the null lines denull to `None` and
        // keep their place, so the document still ends in two empty lines.
        let mut objs: Arena<Obj<Term>> = Arena::new();
        let a = objs.push(Obj::Term(text_term("a")));
        let n1 = objs.push(Obj::Term(null_term()));
        let n2 = objs.push(Obj::Term(null_term()));
        let doc = RebuildDoc {
            lines: vec![a, n1, n2],
            objs,
            fixes: Arena::new(),
        };
        let out = denull(&doc, &Arena::new());
        assert!(matches!(out.lines[..], [Some(_), None, None]));
    }

    #[test]
    fn denull_fix_arena_folds_bottom_up() {
        // Fix(Comp(Text "a", Text "", pad=true)): the empty right vanishes and
        // the fix survives as its left.
        let mut objs: Arena<Obj<Term>> = Arena::new();
        let mut fixes: Arena<Fix<Term>> = Arena::new();
        let fa = fixes.push(Fix::Term(text_term("a")));
        let fe = fixes.push(Fix::Term(text_term("")));
        let fc = fixes.push(Fix::Comp(fa, fe, Pad::Padded));
        let root = objs.push(Obj::Fix(fc));
        let doc = RebuildDoc {
            lines: vec![root],
            objs,
            fixes,
        };
        let out = denull(&doc, &Arena::new());
        let [Some(root)] = out.lines[..] else {
            panic!("expected a single line");
        };
        let Obj::Fix(fix1) = out.objs[root] else {
            panic!("expected a fix object");
        };
        let Fix::Term(t) = &out.fixes[fix1] else {
            panic!("expected the fix to survive as its left term");
        };
        assert_eq!(t.text, "a");
    }
}
