use super::layout::Attr;
use crate::list::List;
use std::cell::Cell;
use std::fmt;

// First intermediate representation: Broken
#[derive(Debug)]
pub enum Broken<'a> {
    Null,
    Text(&'a str),
    Fix(&'a Broken<'a>),
    Grp(&'a Broken<'a>),
    Seq(bool, &'a Broken<'a>),
    Nest(&'a Broken<'a>),
    Pack(&'a Broken<'a>),
    Line(&'a Broken<'a>, &'a Broken<'a>),
    Comp(&'a Broken<'a>, &'a Broken<'a>, Attr),
}

// Second intermediate representation: Edsl
#[derive(Debug)]
pub enum Edsl<'a> {
    Null,
    Text(&'a str),
    Fix(&'a Edsl<'a>),
    Grp(&'a Edsl<'a>),
    Seq(&'a Edsl<'a>),
    Nest(&'a Edsl<'a>),
    Pack(&'a Edsl<'a>),
    Line(&'a Edsl<'a>, &'a Edsl<'a>),
    Comp(&'a Edsl<'a>, &'a Edsl<'a>, Attr),
}

// Third intermediate representation: Serial
#[derive(Debug)]
pub enum Serial<'a> {
    Next(&'a SerialTerm<'a>, &'a SerialComp<'a>, &'a Serial<'a>),
    Last(&'a SerialTerm<'a>, &'a Serial<'a>),
    Past,
}

#[derive(Debug)]
pub enum SerialTerm<'a> {
    Null,
    Text(&'a str),
    Nest(&'a SerialTerm<'a>),
    Pack(u64, &'a SerialTerm<'a>),
}

#[derive(Debug)]
pub enum SerialComp<'a> {
    Line,
    Comp(Attr),
    Grp(u64, &'a SerialComp<'a>),
    Seq(u64, &'a SerialComp<'a>),
}

// Fourth intermediate representation: LinearDoc
#[derive(Debug)]
pub enum LinearDoc<'a> {
    Nil,
    Cons(&'a LinearObj<'a>, &'a LinearDoc<'a>),
}

#[derive(Debug)]
pub enum LinearObj<'a> {
    Next(&'a LinearTerm<'a>, &'a LinearComp<'a>, &'a LinearObj<'a>),
    Last(&'a LinearTerm<'a>),
}

#[derive(Debug)]
pub enum LinearTerm<'a> {
    Null,
    Text(&'a str),
    Nest(&'a LinearTerm<'a>),
    Pack(u64, &'a LinearTerm<'a>),
}

#[derive(Debug)]
pub enum LinearComp<'a> {
    Comp(Attr),
    Grp(u64, &'a LinearComp<'a>),
    Seq(u64, &'a LinearComp<'a>),
}

// Fifth intermediate representation: FixedDoc
#[derive(Debug)]
pub enum FixedDoc<'a> {
    Eod,
    Break(&'a FixedObj<'a>, &'a FixedDoc<'a>),
}

#[derive(Debug)]
pub enum FixedObj<'a> {
    Next(&'a FixedItem<'a>, &'a FixedComp<'a>, &'a FixedObj<'a>),
    Last(&'a FixedItem<'a>),
}

#[derive(Debug)]
pub enum FixedItem<'a> {
    Fix(&'a FixedFix<'a>),
    Term(&'a FixedTerm<'a>),
}

#[derive(Debug)]
pub enum FixedTerm<'a> {
    Null,
    Text(&'a str),
    Nest(&'a FixedTerm<'a>),
    Pack(u64, &'a FixedTerm<'a>),
}

#[derive(Debug)]
pub enum FixedComp<'a> {
    Comp(bool),
    Grp(u64, &'a FixedComp<'a>),
    Seq(u64, &'a FixedComp<'a>),
}

#[derive(Debug)]
pub enum FixedFix<'a> {
    Next(&'a FixedTerm<'a>, &'a FixedComp<'a>, &'a FixedFix<'a>),
    Last(&'a FixedTerm<'a>),
}

// Property type for graph algorithms
#[derive(Debug, Copy, Clone)]
pub enum Property<T> {
    Grp(T),
    Seq(T),
}

// Graph representation types
#[derive(Debug)]
pub enum GraphDoc<'a> {
    Eod,
    Break(
        &'a List<'a, &'a GraphNode<'a>>,
        &'a List<'a, bool>,
        &'a GraphDoc<'a>,
    ),
}

#[derive(Debug)]
pub struct GraphNode<'a> {
    pub index: u64,
    pub term: &'a GraphTerm<'a>,
    pub ins_head: Cell<Option<&'a GraphEdge<'a>>>,
    pub ins_tail: Cell<Option<&'a GraphEdge<'a>>>,
    pub outs_head: Cell<Option<&'a GraphEdge<'a>>>,
    pub outs_tail: Cell<Option<&'a GraphEdge<'a>>>,
}

#[derive(Debug)]
pub struct GraphEdge<'a> {
    pub prop: Property<()>,
    pub ins_next: Cell<Option<&'a GraphEdge<'a>>>,
    pub ins_prev: Cell<Option<&'a GraphEdge<'a>>>,
    pub outs_next: Cell<Option<&'a GraphEdge<'a>>>,
    pub outs_prev: Cell<Option<&'a GraphEdge<'a>>>,
    pub source: Cell<&'a GraphNode<'a>>,
    pub target: Cell<&'a GraphNode<'a>>,
}

#[derive(Debug)]
pub enum GraphTerm<'a> {
    Null,
    Text(&'a str),
    Fix(&'a GraphFix<'a>),
    Nest(&'a GraphTerm<'a>),
    Pack(u64, &'a GraphTerm<'a>),
}

#[derive(Debug)]
pub enum GraphFix<'a> {
    Last(&'a GraphTerm<'a>),
    Next(&'a GraphTerm<'a>, &'a GraphFix<'a>, bool),
}

// Sixth intermediate representation: RebuildDoc
#[derive(Debug)]
pub enum RebuildDoc<'a> {
    Eod,
    Break(&'a RebuildObj<'a>, &'a RebuildDoc<'a>),
}

#[derive(Debug)]
pub enum RebuildObj<'a> {
    Term(&'a RebuildTerm<'a>),
    Fix(&'a RebuildFix<'a>),
    Grp(&'a RebuildObj<'a>),
    Seq(&'a RebuildObj<'a>),
    Comp(&'a RebuildObj<'a>, &'a RebuildObj<'a>, bool),
}

#[derive(Debug)]
pub enum RebuildFix<'a> {
    Term(&'a RebuildTerm<'a>),
    Comp(&'a RebuildFix<'a>, &'a RebuildFix<'a>, bool),
}

#[derive(Debug)]
pub enum RebuildTerm<'a> {
    Null,
    Text(&'a str),
    Nest(&'a RebuildTerm<'a>),
    Pack(u64, &'a RebuildTerm<'a>),
}

#[derive(Copy, Clone)]
pub struct RebuildCont<'a>(
    pub &'a dyn Fn(&'a bumpalo::Bump, &'a RebuildObj<'a>) -> &'a RebuildObj<'a>,
);

impl<'a> fmt::Debug for RebuildCont<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "RebuildCont(<fn>)")
    }
}

// Seventh intermediate representation: DenullDoc
#[derive(Debug)]
pub enum DenullDoc<'a> {
    Eod,
    Line(&'a DenullObj<'a>),
    Empty(&'a DenullDoc<'a>),
    Break(&'a DenullObj<'a>, &'a DenullDoc<'a>),
}

#[derive(Debug)]
pub enum DenullObj<'a> {
    Term(&'a DenullTerm<'a>),
    Fix(&'a DenullFix<'a>),
    Grp(&'a DenullObj<'a>),
    Seq(&'a DenullObj<'a>),
    Comp(&'a DenullObj<'a>, &'a DenullObj<'a>, bool),
}

#[derive(Debug)]
pub enum DenullFix<'a> {
    Term(&'a DenullTerm<'a>),
    Comp(&'a DenullFix<'a>, &'a DenullFix<'a>, bool),
}

#[derive(Debug)]
pub enum DenullTerm<'a> {
    Text(&'a str),
    Nest(&'a DenullTerm<'a>),
    Pack(u64, &'a DenullTerm<'a>),
}

// Eighth intermediate representation: FinalDoc
#[derive(Debug)]
pub enum FinalDoc<'a> {
    Eod,
    Empty(&'a FinalDoc<'a>),
    Break(&'a FinalDocObj<'a>, &'a FinalDoc<'a>),
    Line(&'a FinalDocObj<'a>),
}

#[derive(Debug)]
pub enum FinalDocObj<'a> {
    Text(&'a str),
    Fix(&'a FinalDocObjFix<'a>),
    Grp(&'a FinalDocObj<'a>),
    Seq(&'a FinalDocObj<'a>),
    Nest(&'a FinalDocObj<'a>),
    Pack(u64, &'a FinalDocObj<'a>),
    Comp(&'a FinalDocObj<'a>, &'a FinalDocObj<'a>, bool),
}

#[derive(Debug)]
pub enum FinalDocObjFix<'a> {
    Text(&'a str),
    Comp(&'a FinalDocObjFix<'a>, &'a FinalDocObjFix<'a>, bool),
}

// Type aliases for complex function signatures
pub type GraphTermList<'b> = &'b List<'b, &'b GraphTerm<'b>>;
pub type U64List<'b> = &'b List<'b, u64>;
pub type PropertyListList<'b> = &'b List<'b, &'b List<'b, Property<()>>>;
pub type TermTransformer<'b> =
    &'b dyn Fn(&'b bumpalo::Bump, GraphTermList<'b>) -> GraphTermList<'b>;
pub type U64Transformer<'b> = &'b dyn Fn(&'b bumpalo::Bump, U64List<'b>) -> U64List<'b>;
pub type PropertyTransformer<'b> =
    &'b dyn Fn(&'b bumpalo::Bump, PropertyListList<'b>) -> PropertyListList<'b>;

pub type TopologyResult<'b> = (GraphTermList<'b>, U64List<'b>, PropertyListList<'b>);
