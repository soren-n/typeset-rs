//! denull: RebuildDoc → DenullDoc (remove null identities)
//!
//! Drops `Null`/empty-text terms, collapsing objects that reduce to nothing,
//! and strips each surviving term's nest/pack chain into a flat prop list.
//! Both input and output are flat postorder arenas (children precede parents),
//! so the object and fix walks are plain forward folds: by the time a node is
//! visited its children's results are already computed.

use crate::compiler::types::{
    Arena, DFixId, DObjId, DenullDoc, DenullFix, DenullObj, DenullRow, DenullTerm, IdVec, PathNode,
    Prop, Range, RebuildDoc, RebuildFix, RebuildObj, Term, TermLeaf,
};

/// Result of denulling an object: nothing survived (`None`); an object
/// survived (`Some`); or everything to the left of a composition was dropped,
/// leaving a surviving object plus the accumulated pad (`NextNone`).
#[derive(Copy, Clone)]
enum Res<Id> {
    None,
    Some(Id),
    NextNone(bool, Id),
}

/// Remove null identities.
pub fn denull<'a>(doc: &RebuildDoc<'a>, paths: &Arena<PathNode>) -> DenullDoc<'a> {
    // Every input node yields at most one output node, so the input sizes are
    // exact capacity bounds.
    let mut objs: Arena<DenullObj<'a>> = Arena::with_capacity(doc.objs.len());
    let mut fixes: Arena<DenullFix<'a>> = Arena::with_capacity(doc.fixes.len());
    // The shared prop buffer every surviving term's props range indexes, and
    // the memo of already-materialized paths: terms sharing a path (sibling
    // leaves under the same wrappers) share one materialization, so the
    // buffer is O(path arena), not O(terms × depth).
    let mut props: Vec<Prop> = Vec::new();
    let mut memo: IdVec<PathNode, Option<Range<Prop>>> = IdVec::filled(None, paths.len());

    // Fold the fixed-object arena bottom-up (forward, children first).
    let mut fix_res: IdVec<RebuildFix<'a>, Res<DFixId<'a>>> = IdVec::with_capacity(doc.fixes.len());
    for (_, node) in doc.fixes.iter() {
        let res = match *node {
            RebuildFix::Term(term) => match strip_term(&mut props, &mut memo, paths, term) {
                None => Res::None,
                Some(term1) => Res::Some(fixes.push(DenullFix::Term(term1))),
            },
            RebuildFix::Comp(left, right, l_pad) => comp_res(
                fix_res[left],
                fix_res[right],
                l_pad,
                |left1, right1, pad| fixes.push(DenullFix::Comp(left1, right1, pad)),
            ),
        };
        fix_res.push(res);
    }

    // Fold the object arena bottom-up the same way.
    let mut obj_res: IdVec<RebuildObj<'a>, Res<DObjId<'a>>> = IdVec::with_capacity(doc.objs.len());
    for (_, node) in doc.objs.iter() {
        let res = match *node {
            RebuildObj::Term(term) => match strip_term(&mut props, &mut memo, paths, term) {
                None => Res::None,
                Some(term1) => Res::Some(objs.push(DenullObj::Term(term1))),
            },
            RebuildObj::Fix(fix) => match fix_res[fix] {
                Res::None => Res::None,
                Res::Some(fix1) | Res::NextNone(_, fix1) => {
                    Res::Some(objs.push(DenullObj::Fix(fix1)))
                }
            },
            RebuildObj::Grp(obj1) => wrap_obj(&mut objs, obj_res[obj1], DenullObj::Grp),
            RebuildObj::Seq(obj1) => wrap_obj(&mut objs, obj_res[obj1], DenullObj::Seq),
            RebuildObj::Comp(left, right, l_pad) => comp_res(
                obj_res[left],
                obj_res[right],
                l_pad,
                |left1, right1, pad| objs.push(DenullObj::Comp(left1, right1, pad)),
            ),
        };
        obj_res.push(res);
    }

    // Emit the spine rows in document order: a line whose object survived is a
    // Break, an emptied line is an Empty. Then resolve the tail: the final
    // surviving object is a Line row, and a final emptied line is the document
    // end (no row at all).
    let mut rows: Vec<DenullRow> = doc
        .lines
        .iter()
        .map(|&root| match obj_res[root] {
            Res::None => DenullRow::Empty,
            Res::Some(obj1) | Res::NextNone(_, obj1) => DenullRow::Break(obj1),
        })
        .collect();
    match rows.last() {
        Some(DenullRow::Empty) => {
            rows.pop();
        }
        Some(DenullRow::Break(obj1)) => {
            let last = DenullRow::Line(*obj1);
            *rows.last_mut().expect("non-empty rows") = last;
        }
        _ => {}
    }

    DenullDoc {
        rows,
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
    l_pad: bool,
    mut comp: impl FnMut(Id, Id, bool) -> Id,
) -> Res<Id> {
    match (left, right) {
        (Res::None, Res::None) => Res::None,
        (Res::None, Res::Some(right1)) => Res::NextNone(l_pad, right1),
        (Res::None, Res::NextNone(r_pad, right1)) => Res::NextNone(l_pad || r_pad, right1),
        (Res::Some(left1), Res::None) => Res::Some(left1),
        (Res::Some(left1), Res::Some(right1)) => Res::Some(comp(left1, right1, l_pad)),
        (Res::Some(left1), Res::NextNone(r_pad, right1)) => {
            Res::Some(comp(left1, right1, l_pad || r_pad))
        }
        // A composition's left operand never denulls to NextNone.
        (Res::NextNone(..), _) => unreachable!("Invariant"),
    }
}

/// Wraps a denulled object in a `Grp` or `Seq` (whichever `ctor` builds),
/// propagating the "nothing survived" result unchanged. A surviving `NextNone`
/// collapses to `Some` — the dropped-left pad is discarded at a wrapper.
fn wrap_obj<'a>(
    objs: &mut Arena<DenullObj<'a>>,
    val: Res<DObjId<'a>>,
    ctor: fn(DObjId<'a>) -> DenullObj<'a>,
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

    /// Far past where a native-stack recursion could survive; with flat arenas
    /// the folds are plain loops, so this guards sizing behavior only.
    const DEEP: usize = 50_000;

    #[test]
    fn denull_handles_deep_comp_object() {
        // Right-nested Comp chain: Comp(Term, Comp(Term, ... Term)). Each left
        // operand is a surviving Term, so the whole object survives.
        let mut objs: Arena<RebuildObj> = Arena::new();
        let mut cur = objs.push(RebuildObj::Term(text_term("z")));
        for _ in 0..DEEP {
            let left = objs.push(RebuildObj::Term(text_term("y")));
            cur = objs.push(RebuildObj::Comp(left, cur, false));
        }
        let doc = RebuildDoc {
            lines: vec![cur],
            objs,
            fixes: Arena::new(),
        };
        let out = denull(&doc, &Arena::new());
        // Count the surviving comps in the single line.
        let [DenullRow::Line(root)] = out.rows[..] else {
            panic!("expected a single line");
        };
        let mut count = 0usize;
        let mut walk = root;
        while let DenullObj::Comp(_left, right, _pad) = out.objs[walk] {
            count += 1;
            walk = right;
        }
        assert_eq!(count, DEEP);
    }

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
        let mut objs: Arena<RebuildObj> = Arena::new();
        let root = objs.push(RebuildObj::Term(term));
        let doc = RebuildDoc {
            lines: vec![root],
            objs,
            fixes: Arena::new(),
        };
        let out = denull(&doc, &paths);
        let [DenullRow::Line(root)] = out.rows[..] else {
            panic!("expected a single line");
        };
        let DenullObj::Term(t) = &out.objs[root] else {
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
        let mut objs: Arena<RebuildObj> = Arena::new();
        let n1 = objs.push(RebuildObj::Term(null_term()));
        let t = objs.push(RebuildObj::Term(text_term("x")));
        let inner = objs.push(RebuildObj::Comp(n1, t, true));
        let a = objs.push(RebuildObj::Term(text_term("a")));
        let root = objs.push(RebuildObj::Comp(a, inner, false));
        let doc = RebuildDoc {
            lines: vec![root],
            objs,
            fixes: Arena::new(),
        };
        let out = denull(&doc, &Arena::new());
        let [DenullRow::Line(root)] = out.rows[..] else {
            panic!("expected a single line");
        };
        let DenullObj::Comp(_, _, pad) = out.objs[root] else {
            panic!("expected a comp line");
        };
        assert!(pad, "the dropped left's pad must merge into the comp");
    }

    #[test]
    fn denull_handles_long_doc_spine() {
        let mut objs: Arena<RebuildObj> = Arena::new();
        let mut lines = Vec::new();
        for _ in 0..DEEP {
            lines.push(objs.push(RebuildObj::Term(text_term("x"))));
        }
        let doc = RebuildDoc {
            lines,
            objs,
            fixes: Arena::new(),
        };
        let out = denull(&doc, &Arena::new());
        assert_eq!(out.rows.len(), DEEP);
        assert!(matches!(out.rows.last(), Some(DenullRow::Line(_))));
        let breaks = out
            .rows
            .iter()
            .filter(|r| matches!(r, DenullRow::Break(_)))
            .count();
        assert_eq!(breaks, DEEP - 1);
    }

    #[test]
    fn denull_emptied_lines_become_empty_rows_and_a_trailing_one_ends_the_doc() {
        // Lines: [Text "a", Null, Null]. The first null line becomes an Empty
        // row; the trailing one is the document end and emits no row.
        let mut objs: Arena<RebuildObj> = Arena::new();
        let a = objs.push(RebuildObj::Term(text_term("a")));
        let n1 = objs.push(RebuildObj::Term(null_term()));
        let n2 = objs.push(RebuildObj::Term(null_term()));
        let doc = RebuildDoc {
            lines: vec![a, n1, n2],
            objs,
            fixes: Arena::new(),
        };
        let out = denull(&doc, &Arena::new());
        assert!(matches!(
            out.rows[..],
            [DenullRow::Break(_), DenullRow::Empty]
        ));
    }

    #[test]
    fn denull_fix_arena_folds_bottom_up() {
        // Fix(Comp(Text "a", Text "", pad=true)): the empty right vanishes and
        // the fix survives as its left.
        let mut objs: Arena<RebuildObj> = Arena::new();
        let mut fixes: Arena<RebuildFix> = Arena::new();
        let fa = fixes.push(RebuildFix::Term(text_term("a")));
        let fe = fixes.push(RebuildFix::Term(text_term("")));
        let fc = fixes.push(RebuildFix::Comp(fa, fe, true));
        let root = objs.push(RebuildObj::Fix(fc));
        let doc = RebuildDoc {
            lines: vec![root],
            objs,
            fixes,
        };
        let out = denull(&doc, &Arena::new());
        let [DenullRow::Line(root)] = out.rows[..] else {
            panic!("expected a single line");
        };
        let DenullObj::Fix(fix1) = out.objs[root] else {
            panic!("expected a fix object");
        };
        let DenullFix::Term(t) = &out.fixes[fix1] else {
            panic!("expected the fix to survive as its left term");
        };
        assert_eq!(t.text, "a");
    }
}
