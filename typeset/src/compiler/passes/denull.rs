//! denull: RebuildDoc → DenullDoc (remove null identities)
//!
//! Drops `Null`/empty-text terms, collapsing objects that reduce to nothing,
//! and strips each surviving term's nest/pack chain into a flat prop list.
//! Both input and output are flat postorder arenas (children precede parents),
//! so the object and fix walks are plain forward folds: by the time a node is
//! visited its children's results are already computed. The output is owned —
//! no bump arena backs it.

use crate::compiler::types::{
    DFixId, DObjId, DenullDoc, DenullFix, DenullObj, DenullRow, DenullTerm, NO_PATH, PathNode,
    Prop, Props, RebuildDoc, RebuildFix, RebuildObj, Term, TermLeaf, push_node,
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
pub fn denull<'a>(doc: &RebuildDoc<'a>, paths: &[PathNode]) -> DenullDoc<'a> {
    // Every input node yields at most one output node, so the input sizes are
    // exact capacity bounds.
    let mut objs: Vec<DenullObj<'a>> = Vec::with_capacity(doc.objs.len());
    let mut fixes: Vec<DenullFix<'a>> = Vec::with_capacity(doc.fixes.len());
    // The shared prop buffer every surviving term's props range indexes, and
    // the memo of already-materialized paths: terms sharing a path (sibling
    // leaves under the same wrappers) share one materialization, so the
    // buffer is O(path arena), not O(terms × depth).
    let mut props: Vec<Prop> = Vec::new();
    let mut memo: Vec<Option<Props>> = vec![None; paths.len()];

    // Fold the fixed-object arena bottom-up (forward, children first).
    let mut fix_res: Vec<Res<DFixId>> = Vec::with_capacity(doc.fixes.len());
    for node in &doc.fixes {
        let res = match node {
            RebuildFix::Term(term) => match strip_term(&mut props, &mut memo, paths, *term) {
                None => Res::None,
                Some(term1) => Res::Some(push_node(&mut fixes, DenullFix::Term(term1))),
            },
            RebuildFix::Comp(left, right, l_pad) => comp_res(
                fix_res[*left as usize],
                fix_res[*right as usize],
                *l_pad,
                |left1, right1, pad| push_node(&mut fixes, DenullFix::Comp(left1, right1, pad)),
            ),
        };
        fix_res.push(res);
    }

    // Fold the object arena bottom-up the same way.
    let mut obj_res: Vec<Res<DObjId>> = Vec::with_capacity(doc.objs.len());
    for node in &doc.objs {
        let res = match node {
            RebuildObj::Term(term) => match strip_term(&mut props, &mut memo, paths, *term) {
                None => Res::None,
                Some(term1) => Res::Some(push_node(&mut objs, DenullObj::Term(term1))),
            },
            RebuildObj::Fix(fix) => match fix_res[*fix as usize] {
                Res::None => Res::None,
                Res::Some(fix1) | Res::NextNone(_, fix1) => {
                    Res::Some(push_node(&mut objs, DenullObj::Fix(fix1)))
                }
            },
            RebuildObj::Grp(obj1) => wrap_obj(&mut objs, obj_res[*obj1 as usize], DenullObj::Grp),
            RebuildObj::Seq(obj1) => wrap_obj(&mut objs, obj_res[*obj1 as usize], DenullObj::Seq),
            RebuildObj::Comp(left, right, l_pad) => comp_res(
                obj_res[*left as usize],
                obj_res[*right as usize],
                *l_pad,
                |left1, right1, pad| push_node(&mut objs, DenullObj::Comp(left1, right1, pad)),
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
        .map(|&root| match obj_res[root as usize] {
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
    objs: &mut Vec<DenullObj<'a>>,
    val: Res<DObjId>,
    ctor: fn(DObjId) -> DenullObj<'a>,
) -> Res<DObjId> {
    match val {
        Res::None => Res::None,
        Res::Some(obj) | Res::NextNone(_, obj) => Res::Some(push_node(objs, ctor(obj))),
    }
}

/// Denulls a term: `Null` and empty text vanish (wrappers and all); otherwise
/// the term's wrapper path is materialized outermost-first into the shared
/// prop buffer (memoized per path id — sibling terms under the same wrappers
/// share one materialization) and the term records the range.
fn strip_term<'a>(
    props: &mut Vec<Prop>,
    memo: &mut [Option<Props>],
    paths: &[PathNode],
    term: Term<'a>,
) -> Option<DenullTerm<'a>> {
    let text = match term.leaf {
        TermLeaf::Null | TermLeaf::Text("") => return None,
        TermLeaf::Text(data) => data,
    };
    let range = if term.path == NO_PATH {
        Props { start: 0, end: 0 }
    } else if let Some(range) = memo[term.path as usize] {
        range
    } else {
        let start = props.len();
        let mut cur = term.path;
        while cur != NO_PATH {
            props.push(paths[cur as usize].prop);
            cur = paths[cur as usize].parent;
        }
        // The path walk yields innermost-first; prop lists are outermost-first.
        props[start..].reverse();
        let range = Props {
            start: start as u32,
            end: props.len() as u32,
        };
        memo[term.path as usize] = Some(range);
        range
    };
    Some(DenullTerm { props: range, text })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::RObjId;

    fn text_term(text: &'static str) -> Term<'static> {
        Term {
            path: NO_PATH,
            leaf: TermLeaf::Text(text),
        }
    }

    fn null_term() -> Term<'static> {
        Term {
            path: NO_PATH,
            leaf: TermLeaf::Null,
        }
    }

    /// Far past where a native-stack recursion could survive; with flat arenas
    /// the folds are plain loops, so this now guards the row/term walks only.
    const DEEP: usize = 50_000;

    #[test]
    fn denull_handles_deep_comp_object() {
        // Right-nested Comp chain: Comp(Term, Comp(Term, ... Term)). Each left
        // operand is a surviving Term, so the whole object survives.
        let mut objs: Vec<RebuildObj> = Vec::new();
        let mut cur = push_node(&mut objs, RebuildObj::Term(text_term("z")));
        for _ in 0..DEEP {
            let left = push_node(&mut objs, RebuildObj::Term(text_term("y")));
            cur = push_node(&mut objs, RebuildObj::Comp(left, cur, false));
        }
        let doc = RebuildDoc {
            lines: vec![cur],
            objs,
            fixes: Vec::new(),
        };
        let out = denull(&doc, &[]);
        // Count the surviving comps in the single line.
        let [DenullRow::Line(root)] = out.rows[..] else {
            panic!("expected a single line");
        };
        let mut count = 0usize;
        let mut walk = root;
        while let DenullObj::Comp(_left, right, _pad) = out.objs[walk as usize] {
            count += 1;
            walk = right;
        }
        assert_eq!(count, DEEP);
    }

    #[test]
    fn denull_strips_deep_nest_term_to_props() {
        let mut paths: Vec<PathNode> = Vec::new();
        let mut path = NO_PATH;
        for _ in 0..DEEP {
            let id = paths.len() as u32;
            paths.push(PathNode {
                prop: Prop::Nest,
                parent: path,
            });
            path = id;
        }
        let term = Term {
            path,
            leaf: TermLeaf::Text("x"),
        };
        let mut objs: Vec<RebuildObj> = Vec::new();
        let root = push_node(&mut objs, RebuildObj::Term(term));
        let doc = RebuildDoc {
            lines: vec![root],
            objs,
            fixes: Vec::new(),
        };
        let out = denull(&doc, &paths);
        let [DenullRow::Line(root)] = out.rows[..] else {
            panic!("expected a single line");
        };
        let DenullObj::Term(t) = &out.objs[root as usize] else {
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
        let mut objs: Vec<RebuildObj> = Vec::new();
        let n1 = push_node(&mut objs, RebuildObj::Term(null_term()));
        let t = push_node(&mut objs, RebuildObj::Term(text_term("x")));
        let inner = push_node(&mut objs, RebuildObj::Comp(n1, t, true));
        let a = push_node(&mut objs, RebuildObj::Term(text_term("a")));
        let root = push_node(&mut objs, RebuildObj::Comp(a, inner, false));
        let doc = RebuildDoc {
            lines: vec![root],
            objs,
            fixes: Vec::new(),
        };
        let out = denull(&doc, &[]);
        let [DenullRow::Line(root)] = out.rows[..] else {
            panic!("expected a single line");
        };
        let DenullObj::Comp(_, _, pad) = out.objs[root as usize] else {
            panic!("expected a comp line");
        };
        assert!(pad, "the dropped left's pad must merge into the comp");
    }

    #[test]
    fn denull_handles_long_doc_spine() {
        let mut objs: Vec<RebuildObj> = Vec::new();
        let mut lines: Vec<RObjId> = Vec::new();
        for _ in 0..DEEP {
            lines.push(push_node(&mut objs, RebuildObj::Term(text_term("x"))));
        }
        let doc = RebuildDoc {
            lines,
            objs,
            fixes: Vec::new(),
        };
        let out = denull(&doc, &[]);
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
        let mut objs: Vec<RebuildObj> = Vec::new();
        let a = push_node(&mut objs, RebuildObj::Term(text_term("a")));
        let n1 = push_node(&mut objs, RebuildObj::Term(null_term()));
        let n2 = push_node(&mut objs, RebuildObj::Term(null_term()));
        let doc = RebuildDoc {
            lines: vec![a, n1, n2],
            objs,
            fixes: Vec::new(),
        };
        let out = denull(&doc, &[]);
        assert!(matches!(
            out.rows[..],
            [DenullRow::Break(_), DenullRow::Empty]
        ));
    }

    #[test]
    fn denull_fix_arena_folds_bottom_up() {
        // Fix(Comp(Text "a", Text "", pad=true)): the empty right vanishes and
        // the fix survives as its left.
        let mut objs: Vec<RebuildObj> = Vec::new();
        let mut fixes: Vec<RebuildFix> = Vec::new();
        let fa = push_node(&mut fixes, RebuildFix::Term(text_term("a")));
        let fe = push_node(&mut fixes, RebuildFix::Term(text_term("")));
        let fc = push_node(&mut fixes, RebuildFix::Comp(fa, fe, true));
        let root = push_node(&mut objs, RebuildObj::Fix(fc));
        let doc = RebuildDoc {
            lines: vec![root],
            objs,
            fixes,
        };
        let out = denull(&doc, &[]);
        let [DenullRow::Line(root)] = out.rows[..] else {
            panic!("expected a single line");
        };
        let DenullObj::Fix(fix1) = out.objs[root as usize] else {
            panic!("expected a fix object");
        };
        let DenullFix::Term(t) = &out.fixes[fix1 as usize] else {
            panic!("expected the fix to survive as its left term");
        };
        assert_eq!(t.text, "a");
    }
}
