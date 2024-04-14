use std::{
  cell::Cell,
  option::Option,
  cmp::max,
  fmt
};
use bumpalo::Bump;

use crate::{
  util::compose,
  order::total,
  list::{self as _list, List},
  map::{self as _map, Map}
};

// EDSL syntax
#[derive(Debug, Copy, Clone)]
pub struct Attr {
  pad: bool,
  fix: bool
}

#[derive(Debug, Clone)]
pub enum Layout {
  Null,
  Text(String),
  Fix(Box<Layout>),
  Grp(Box<Layout>),
  Seq(Box<Layout>),
  Nest(Box<Layout>),
  Pack(Box<Layout>),
  Line(Box<Layout>, Box<Layout>),
  Comp(Box<Layout>, Box<Layout>, Attr)
}

impl fmt::Display for Layout {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    fn _visit(
      layout: Box<Layout>
    ) -> String {
      match layout {
        box Layout::Null =>
          "Null".to_string(),
        box Layout::Text(data) =>
          format!("(Text \"{}\"", data),
        box Layout::Fix(layout1) => {
          let layout_s = _visit(layout1);
          format!("(Fix {})", layout_s)
        }
        box Layout::Grp(layout1) => {
          let layout_s = _visit(layout1);
          format!("(Grp {})", layout_s)
        }
        box Layout::Seq(layout1) => {
          let layout_s = _visit(layout1);
          format!("(Seq {})", layout_s)
        }
        box Layout::Nest(layout1) => {
          let layout_s = _visit(layout1);
          format!("(Nest {})", layout_s)
        }
        box Layout::Pack(layout1) => {
          let layout_s = _visit(layout1);
          format!("(Pack {})", layout_s)
        }
        box Layout::Line(left, right) => {
          let left_s = _visit(left);
          let right_s = _visit(right);
          format!("(Line {} {})", left_s, right_s)
        }
        box Layout::Comp(left, right, attr) => {
          let left_s = _visit(left);
          let right_s = _visit(right);
          format!("(Comp {} {} {} {})", left_s, right_s, attr.pad, attr.fix)
        }
      }
    }
    write!(f, "{}", _visit(Box::new(self.clone())))
  }
}

/// Constructs a new Null layout.
///
/// Null layouts are literals and are the neutral elements of layout compositions.
///
/// # Examples
/// ```
/// use typeset::null;
///
/// let layout = null();
/// ```
pub fn null() -> Box<Layout> {
  Box::new(Layout::Null)
}

/// Constructs a new Text layout.
///
/// Text layouts are literals and basic elements of layout compositions.
///
/// # Examples
/// ```
/// use typeset::text;
///
/// let layout = text("foobar".to_string());
/// ```
pub fn text(
  data: String
) -> Box<Layout> {
  Box::new(Layout::Text(data))
}

/// Constructs a new Fix layout.
///
/// Fix layouts are modal layouts that will prevent compositions under them from being broken into newlines during rendering.
///
/// # Examples
/// ```
/// use typeset::{text, comp, fix};
///
/// let layout = fix(comp(
///   text("foo".to_string()),
///   text("bar".to_string()),
///   false, false
/// ));
/// ```
pub fn fix(
  layout: Box<Layout>
) -> Box<Layout> {
  Box::new(Layout::Fix(layout))
}

/// Constructs a new Grp layout.
///
/// Grp layouts are modal layouts that will prevent compositions under them from being broken into newlines during rendering, if there are compositions outside of them that could be broken first.
///
/// # Examples
/// ```
/// use typeset::{text, comp, grp};
///
/// let layout = grp(comp(
///   text("foo".to_string()),
///   text("bar".to_string()),
///   false, false
/// ));
/// ```
pub fn grp(
  layout: Box<Layout>
) -> Box<Layout> {
  Box::new(Layout::Grp(layout))
}

/// Constructs a new Seq layout.
///
/// Seq layouts are modal layouts that will ensure that all compositions under them will be broken into newlines during rendering, if any one of the compositions are broken.
///
/// # Examples
/// ```
/// use typeset::{text, comp, seq};
///
/// let layout = seq(comp(
///   text("foo".to_string()),
///   text("bar".to_string()),
///   false, false
/// ));
/// ```
pub fn seq(
  layout: Box<Layout>
) -> Box<Layout> {
  Box::new(Layout::Seq(layout))
}

/// Constructs a new Nest layout.
///
/// Nest layouts are modal layouts that will ensure that indentation will be prefixed to any broken compositions.
///
/// # Examples
/// ```
/// use typeset::{text, comp, nest};
///
/// let layout = nest(comp(
///   text("foo".to_string()),
///   text("bar".to_string()),
///   false, false
/// ));
/// ```
pub fn nest(
  layout: Box<Layout>
) -> Box<Layout> {
  Box::new(Layout::Nest(layout))
}

/// Constructs a new Pack layout.
///
/// Pack layouts are modal layouts that will ensure that indentation will be prefixed to any broken compositions, making sure all the indentations line up with the index of the first character in the pack.
///
/// # Examples
/// ```
/// use typeset::{text, comp, pack};
///
/// let layout = pack(comp(
///   text("foo".to_string()),
///   text("bar".to_string()),
///   false, false
/// ));
/// ```
pub fn pack(
  layout: Box<Layout>
) -> Box<Layout> {
  Box::new(Layout::Pack(layout))
}

/// Constructs a new Line layout.
///
/// Line layouts compose two layouts, ensuring that there is a newline between them.
///
/// # Examples
/// ```
/// use typeset::{text, line};
///
/// let layout = line(
///   text("foo".to_string()),
///   text("bar".to_string())
/// );
/// ```
pub fn line(
  left: Box<Layout>,
  right: Box<Layout>
) -> Box<Layout> {
  Box::new(Layout::Line(left, right))
}

/// Constructs a new Comp layout.
///
/// Comp layouts compose two layouts, either as padded (with whitespace between them) or fixed (the composition can not be broken into a newline) or both.
///
/// # Examples
/// ```
/// use typeset::{text, comp};
///
/// let layout = comp(
///   text("foo".to_string()),
///   text("bar".to_string()),
///   false, false
/// );
/// ```
pub fn comp(
  left: Box<Layout>,
  right: Box<Layout>,
  pad: bool,
  fix: bool
) -> Box<Layout> {
  Box::new(Layout::Comp(left, right, Attr {
    pad: pad,
    fix: fix
  }))
}

#[derive(Debug)]
enum Broken<'a> {
  Null,
  Text(&'a str),
  Fix(&'a Broken<'a>),
  Grp(&'a Broken<'a>),
  Seq(bool, &'a Broken<'a>),
  Nest(&'a Broken<'a>),
  Pack(&'a Broken<'a>),
  Line(&'a Broken<'a>, &'a Broken<'a>),
  Comp(&'a Broken<'a>, &'a Broken<'a>, Attr)
}

#[derive(Debug)]
enum EDSL<'a> {
  Null,
  Text(&'a str),
  Fix(&'a EDSL<'a>),
  Grp(&'a EDSL<'a>),
  Seq(&'a EDSL<'a>),
  Nest(&'a EDSL<'a>),
  Pack(&'a EDSL<'a>),
  Line(&'a EDSL<'a>, &'a EDSL<'a>),
  Comp(&'a EDSL<'a>, &'a EDSL<'a>, Attr)
}

/*
  Collapse broken sequences
*/
fn _broken<'b, 'a: 'b>(
  mem: &'b Bump,
  layout: Box<Layout>
) -> &'b EDSL<'b> {
  fn _mark<'b, 'a: 'b>(
    mem: &'b Bump,
    layout: Box<Layout>
  ) -> &'b Broken<'b> {
    fn _visit<'b, 'a: 'b>(
      mem: &'b Bump,
      layout: Box<Layout>
    ) -> (bool, &'b Broken<'b>) {
      fn _null<'a>(
        mem: &'a Bump
      ) -> &'a Broken<'a> {
        mem.alloc(Broken::Null)
      }
      fn _text<'a>(
        mem: &'a Bump,
        data: &'a str
      ) -> &'a Broken<'a> {
        mem.alloc(Broken::Text(data))
      }
      fn _fix<'a>(
        mem: &'a Bump,
        layout: &'a Broken<'a>
      ) -> &'a Broken<'a> {
        mem.alloc(Broken::Fix(layout))
      }
      fn _grp<'a>(
        mem: &'a Bump,
        layout: &'a Broken<'a>
      ) -> &'a Broken<'a> {
        mem.alloc(Broken::Grp(layout))
      }
      fn _seq<'a>(
        mem: &'a Bump,
        broken: bool,
        layout: &'a Broken<'a>
      ) -> &'a Broken<'a> {
        mem.alloc(Broken::Seq(broken, layout))
      }
      fn _nest<'a>(
        mem: &'a Bump,
        layout: &'a Broken<'a>
      ) -> &'a Broken<'a> {
        mem.alloc(Broken::Nest(layout))
      }
      fn _pack<'a>(
        mem: &'a Bump,
        layout: &'a Broken<'a>
      ) -> &'a Broken<'a> {
        mem.alloc(Broken::Pack(layout))
      }
      fn _line<'a>(
        mem: &'a Bump,
        left: &'a Broken<'a>,
        right: &'a Broken<'a>
      ) -> &'a Broken<'a> {
        mem.alloc(Broken::Line(left, right))
      }
      fn _comp<'a>(
        mem: &'a Bump,
        left: &'a Broken<'a>,
        right: &'a Broken<'a>,
        attr: Attr
      ) -> &'a Broken<'a> {
        mem.alloc(Broken::Comp(left, right, attr))
      }
      match layout {
        box Layout::Null => (false, _null(mem)),
        box Layout::Text(data) => {
          let data1 = mem.alloc_str(data.as_str());
          (false, _text(mem, data1))
        }
        box Layout::Fix(layout1) => {
          let (broken, layout2) = _visit(mem, layout1.clone());
          (broken, _fix(mem, layout2))
        }
        box Layout::Grp(layout1) => {
          let (broken, layout2) = _visit(mem, layout1.clone());
          (broken, _grp(mem, layout2))
        }
        box Layout::Seq(layout1) => {
          let (broken, layout2) = _visit(mem, layout1.clone());
          (broken, _seq(mem, broken, layout2))
        }
        box Layout::Nest(layout1) => {
          let (broken, layout2) = _visit(mem, layout1.clone());
          (broken, _nest(mem, layout2))
        }
        box Layout::Pack(layout1) => {
          let (broken, layout2) = _visit(mem, layout1.clone());
          (broken, _pack(mem, layout2))
        }
        box Layout::Line(left, right) => {
          let (_l_broken, left1) = _visit(mem, left.clone());
          let (_r_broken, right1) = _visit(mem, right.clone());
          (true, _line(mem, left1, right1))
        }
        box Layout::Comp(left, right, attr) => {
          let (l_broken, left1) = _visit(mem, left.clone());
          let (r_broken, right1) = _visit(mem, right.clone());
          let broken = l_broken || r_broken;
          (broken, _comp(mem, left1, right1, attr.clone()))
        }
      }
    }
    let (_break, layout) = _visit(mem, layout);
    layout
  }
  fn _remove<'b, 'a: 'b, R>(
    mem: &'b Bump,
    layout: &'a Broken<'a>,
    broken: bool,
    cont: &'b dyn Fn(&'b Bump, &'b EDSL<'b>) -> R
  ) -> R {
    fn _null<'a>(
      mem: &'a Bump
    ) -> &'a EDSL<'a> {
      mem.alloc(EDSL::Null)
    }
    fn _text<'a>(
      mem: &'a Bump,
      data: &'a str
    ) -> &'a EDSL<'a> {
      mem.alloc(EDSL::Text(data))
    }
    fn _fix<'a>(
      mem: &'a Bump,
      layout: &'a EDSL<'a>
    ) -> &'a EDSL<'a> {
      mem.alloc(EDSL::Fix(layout))
    }
    fn _grp<'a>(
      mem: &'a Bump,
      layout: &'a EDSL<'a>
    ) -> &'a EDSL<'a> {
      mem.alloc(EDSL::Grp(layout))
    }
    fn _seq<'a>(
      mem: &'a Bump,
      layout: &'a EDSL<'a>
    ) -> &'a EDSL<'a> {
      mem.alloc(EDSL::Seq(layout))
    }
    fn _nest<'a>(
      mem: &'a Bump,
      layout: &'a EDSL<'a>
    ) -> &'a EDSL<'a> {
      mem.alloc(EDSL::Nest(layout))
    }
    fn _pack<'a>(
      mem: &'a Bump,
      layout: &'a EDSL<'a>
    ) -> &'a EDSL<'a> {
      mem.alloc(EDSL::Pack(layout))
    }
    fn _line<'a>(
      mem: &'a Bump,
      left: &'a EDSL<'a>,
      right: &'a EDSL<'a>
    ) -> &'a EDSL<'a> {
      mem.alloc(EDSL::Line(left, right))
    }
    fn _comp<'a>(
      mem: &'a Bump,
      left: &'a EDSL<'a>,
      right: &'a EDSL<'a>,
      attr: Attr
    ) -> &'a EDSL<'a> {
      mem.alloc(EDSL::Comp(left, right, attr))
    }
    match layout {
      Broken::Null => cont(mem, _null(mem)),
      Broken::Text(data) => cont(mem, _text(mem, data)),
      Broken::Fix(layout1) =>
        _remove(mem, layout1, false,
          compose(mem, cont, mem.alloc(|mem, layout1|
            _fix(mem, layout1)))),
      Broken::Grp(layout1) =>
        _remove(mem, layout1, false,
          compose(mem, cont, mem.alloc(|mem, layout1|
            _grp(mem, layout1)))),
      Broken::Seq(broken, layout1) =>
        if *broken { _remove(mem, layout1, true, cont) } else {
        _remove(mem, layout1, false,
          compose(mem, cont, mem.alloc(|mem, layout2|
            _seq(mem, layout2))))},
      Broken::Nest(layout1) =>
        _remove(mem, layout1, broken,
          compose(mem, cont, mem.alloc(|mem, layout2|
            _nest(mem, layout2)))),
      Broken::Pack(layout1) =>
        _remove(mem, layout1, broken,
          compose(mem, cont, mem.alloc(|mem, layout2|
            _pack(mem, layout2)))),
      Broken::Line(left, right) =>
        _remove(mem, left, broken, mem.alloc(move |mem, left1|
        _remove(mem, right, broken, mem.alloc(move |mem, right1|
        cont(mem, _line(mem, left1, right1)))))),
      Broken::Comp(left, right, attr) =>
        _remove(mem, left, broken, mem.alloc(move |mem, left1|
        _remove(mem, right, broken, mem.alloc(move |mem, right1|
        if broken && !attr.fix { cont(mem, _line(mem, left1, right1)) }
        else { cont(mem, _comp(mem, left1, right1, *attr)) }))))
    }
  }
  let layout1 = _mark(mem, layout);
  _remove(mem, layout1, false, mem.alloc(|_mem, result| result))
}

#[derive(Debug)]
enum Serial<'a> {
  Next(&'a SerialTerm<'a>, &'a SerialComp<'a>, &'a Serial<'a>),
  Last(&'a SerialTerm<'a>, &'a Serial<'a>),
  Past
}

#[derive(Debug)]
enum SerialTerm<'a> {
  Null,
  Text(&'a str),
  Nest(&'a SerialTerm<'a>),
  Pack(u64, &'a SerialTerm<'a>)
}

#[derive(Debug)]
enum SerialComp<'a> {
  Line,
  Comp(Attr),
  Grp(u64, &'a SerialComp<'a>),
  Seq(u64, &'a SerialComp<'a>)
}

/*
  Serialize in order to normalize
*/
fn _serialize<'b, 'a: 'b>(
  mem: &'b Bump,
  layout: &'a EDSL<'a>
) -> &'b Serial<'b> {
  fn _next<'a>(
    mem: &'a Bump,
    term: &'a SerialTerm<'a>,
    comp: &'a SerialComp<'a>,
    serial: &'a Serial<'a>
  ) -> &'a Serial<'a> {
    mem.alloc(Serial::Next(term, comp, serial))
  }
  fn _last<'a>(
    mem: &'a Bump,
    term: &'a SerialTerm<'a>,
    serial: &'a Serial<'a>
  ) -> &'a Serial<'a> {
    mem.alloc(Serial::Last(term, serial))
  }
  fn _past<'a>(
    mem: &'a Bump
  ) -> &'a Serial<'a> {
    mem.alloc(Serial::Past)
  }
  fn _null<'a>(
    mem: &'a Bump
  ) -> &'a SerialTerm<'a> {
    mem.alloc(SerialTerm::Null)
  }
  fn _text<'a>(
    mem: &'a Bump,
    data: &'a str
  ) -> &'a SerialTerm<'a> {
    mem.alloc(SerialTerm::Text(data))
  }
  fn _nest<'a>(
    mem: &'a Bump,
    term: &'a SerialTerm<'a>
  ) -> &'a SerialTerm<'a> {
    mem.alloc(SerialTerm::Nest(term))
  }
  fn _pack<'a>(
    mem: &'a Bump,
    index: u64,
    term: &'a SerialTerm<'a>
  ) -> &'a SerialTerm<'a> {
    mem.alloc(SerialTerm::Pack(index, term))
  }
  fn _comp<'a>(
    mem: &'a Bump,
    attr: Attr
  ) -> &'a SerialComp<'a> {
    mem.alloc(SerialComp::Comp(attr))
  }
  fn _grp<'a>(
    mem: &'a Bump,
    index: u64,
    comp: &'a SerialComp<'a>
  ) -> &'a SerialComp<'a> {
    mem.alloc(SerialComp::Grp(index, comp))
  }
  fn _seq<'a>(
    mem: &'a Bump,
    index: u64,
    comp: &'a SerialComp<'a>
  ) -> &'a SerialComp<'a> {
    mem.alloc(SerialComp::Seq(index, comp))
  }
  fn __line<'a>(
    mem: &'a Bump,
    term: &'a SerialTerm<'a>,
    serial: &'a Serial<'a>
  ) -> &'a Serial<'a> {
    _next(mem, term, mem.alloc(SerialComp::Line), serial)
  }
  fn __comp<'a>(
    mem: &'a Bump,
    comps: &'a dyn Fn(&'a Bump, &'a SerialComp<'a>) -> &'a SerialComp<'a>,
    attr: Attr,
    term: &'a SerialTerm<'a>,
    serial: &'a Serial<'a>
  ) -> &'a Serial<'a> {
    _next(mem, term, comps(mem, _comp(mem, attr)), serial)
  }
  fn _visit<'b, 'a: 'b, R>(
    mem: &'b Bump,
    i: u64,
    j: u64,
    fixed: bool,
    terms: &'b dyn Fn(&'b Bump, &'b SerialTerm<'b>) -> &'b SerialTerm<'b>,
    comps: &'b dyn Fn(&'b Bump, &'b SerialComp<'b>) -> &'b SerialComp<'b>,
    glue: &'b dyn Fn(&'b Bump, &'b SerialTerm<'b>, &'b Serial<'b>) -> &'b Serial<'b>,
    result: &'b dyn Fn(&'b Bump, &'b Serial<'b>) -> R,
    layout: &'a EDSL<'a>
  ) -> (
    u64, u64, &'b dyn Fn(&'b Bump, &'b Serial<'b>) -> R
  ) {
    match layout {
      EDSL::Null =>
        (i, j, compose(mem, result, mem.alloc(|mem, serial|
        glue(mem, _null(mem), serial)))),
      EDSL::Text(data) =>
        (i, j, compose(mem, result, mem.alloc(|mem, serial|
        glue(mem, terms(mem, _text(mem, data)), serial)))),
      EDSL::Fix(layout1) =>
        _visit(mem, i, j, true, terms, comps, glue, result, layout1),
      EDSL::Grp(layout1) =>
        _visit(
          mem,
          i + 1, j,
          fixed,
          terms,
          compose(mem, comps, mem.alloc(move |mem, comp| _grp(mem, i, comp))),
          glue,
          result,
          layout1
        ),
      EDSL::Seq(layout1) =>
        _visit(
          mem,
          i + 1, j,
          fixed,
          terms,
          compose(mem, comps, mem.alloc(move |mem, comp| _seq(mem, i, comp))),
          glue,
          result,
          layout1
        ),
      EDSL::Nest(layout1) =>
        _visit(
          mem,
          i, j,
          fixed,
          compose(mem, terms, mem.alloc(|mem, term| _nest(mem, term))),
          comps,
          glue,
          result,
          layout1
        ),
      EDSL::Pack(layout1) =>
        _visit(
          mem,
          i, j + 1,
          fixed,
          compose(mem, terms, mem.alloc(move |mem, term| _pack(mem, j, term))),
          comps,
          glue,
          result,
          layout1
        ),
      EDSL::Line(left, right) => {
        let (i1, j1, result1) = _visit(
          mem,
          i, j,
          fixed,
          terms,
          comps,
          mem.alloc(|mem, term, serial| __line(mem, term, serial)),
          result,
          left
        );
        _visit(
          mem, i1, j1, fixed, terms, comps, glue, result1, right
        )
      }
      EDSL::Comp(left, right, attr) => {
        let glue1 = mem.alloc(move |mem, term, serial| {
          let attr1 = Attr {
            pad: attr.pad,
            fix: fixed || attr.fix
          };
          __comp(mem, comps, attr1, term, serial)
        });
        let (i1, j1, result1) = _visit(
          mem, i, j, fixed, terms, comps, glue1, result, left
        );
        _visit(
          mem, i1, j1, fixed, terms, comps, glue, result1, right
        )
      }
    }
  }
  let (_i, _j, result) = _visit(
    mem,
    0, 0,
    false,
    mem.alloc(|_mem, x| x),
    mem.alloc(|_mem, x| x),
    mem.alloc(|mem, term, serial| _last(mem, term, serial)),
    mem.alloc(|_mem, x| x),
    layout
  );
  result(mem, _past(mem))
}

#[derive(Debug)]
enum LinearDoc<'a> {
  Nil,
  Cons(&'a LinearObj<'a>, &'a LinearDoc<'a>)
}

#[derive(Debug)]
enum LinearObj<'a> {
  Next(&'a LinearTerm<'a>, &'a LinearComp<'a>, &'a LinearObj<'a>),
  Last(&'a LinearTerm<'a>)
}

#[derive(Debug)]
enum LinearTerm<'a> {
  Null,
  Text(&'a str),
  Nest(&'a LinearTerm<'a>),
  Pack(u64, &'a LinearTerm<'a>)
}

#[derive(Debug)]
enum LinearComp<'a> {
  Comp(Attr),
  Grp(u64, &'a LinearComp<'a>),
  Seq(u64, &'a LinearComp<'a>)
}

/*
  Lift newlines to spine
*/
fn _linearize<'b, 'a: 'b>(
  mem: &'b Bump,
  serial: &'a Serial<'a>
) -> &'b LinearDoc<'b> {
  fn _nil<'a>(
    mem: &'a Bump
  ) -> &'a LinearDoc<'a> {
    mem.alloc(LinearDoc::Nil)
  }
  fn _cons<'a>(
    mem: &'a Bump,
    obj: &'a LinearObj<'a>,
    doc: &'a LinearDoc<'a>
  ) -> &'a LinearDoc<'a>{
    mem.alloc(LinearDoc::Cons(obj, doc))
  }
  fn _next<'a>(
    mem: &'a Bump,
    comp: &'a LinearTerm<'a>,
    term: &'a LinearComp<'a>,
    obj: &'a LinearObj<'a>
  ) -> &'a LinearObj<'a> {
    mem.alloc(LinearObj::Next(comp, term, obj))
  }
  fn _last<'a>(
    mem: &'a Bump,
    term: &'a LinearTerm<'a>
  ) -> &'a LinearObj<'a> {
    mem.alloc(LinearObj::Last(term))
  }
  fn _null<'a>(
    mem: &'a Bump
  ) -> &'a LinearTerm<'a> {
    mem.alloc(LinearTerm::Null)
  }
  fn _text<'a>(
    mem: &'a Bump,
    data: &'a str
  ) -> &'a LinearTerm<'a> {
    mem.alloc(LinearTerm::Text(data))
  }
  fn _nest<'a>(
    mem: &'a Bump,
    term: &'a LinearTerm<'a>
  ) -> &'a LinearTerm<'a> {
    mem.alloc(LinearTerm::Nest(term))
  }
  fn _pack<'a>(
    mem: &'a Bump,
    index: u64,
    term: &'a LinearTerm<'a>
  ) -> &'a LinearTerm<'a> {
    mem.alloc(LinearTerm::Pack(index, term))
  }
  fn _comp<'a>(
    mem: &'a Bump,
    attr: Attr
  ) -> &'a LinearComp<'a> {
    mem.alloc(LinearComp::Comp(attr))
  }
  fn _grp<'a>(
    mem: &'a Bump,
    index: u64,
    comp: &'a LinearComp<'a>
  ) -> &'a LinearComp<'a> {
    mem.alloc(LinearComp::Grp(index, comp))
  }
  fn _seq<'a>(
    mem: &'a Bump,
    index: u64,
    comp: &'a LinearComp<'a>
  ) -> &'a LinearComp<'a> {
    mem.alloc(LinearComp::Seq(index, comp))
  }
  fn _visit_serial<'b, 'a: 'b, R>(
    mem: &'b Bump,
    serial: &'a Serial<'a>,
    line: &'b dyn Fn(&'b Bump, &'b LinearObj<'b>) -> &'b LinearObj<'b>,
    cont: &'b dyn Fn(&'b Bump, &'b LinearDoc<'b>) -> R
  ) -> R {
    match serial {
      Serial::Next(term, SerialComp::Line, serial1) =>
        _visit_term(mem, term, mem.alloc(move |mem, term1|
        _visit_serial(
          mem,
          serial1,
          mem.alloc(|_mem, obj| obj),
          mem.alloc(move |mem, serial2|
            cont(mem, _cons(mem, line(mem, _last(mem, term1)), serial2))
        )))),
      Serial::Next(term, comp, serial1) =>
        _visit_term(mem, term, mem.alloc(move |mem, term1|
        _visit_comp(mem, comp, mem.alloc(move |mem, comp1|
        _visit_serial(
          mem,
          serial1,
          compose(mem, line, mem.alloc(move |mem, obj|
            _next(mem, term1, comp1, obj))),
          cont
        ))))),
      Serial::Last(term, Serial::Past) =>
        _visit_term(mem, term, mem.alloc(|mem, term1|
        cont(mem, _cons(mem, line(mem, _last(mem, term1)), _nil(mem))))),
      _ => unreachable!("Invariant")
    }
  }
  fn _visit_term<'b, 'a: 'b, R>(
    mem: &'b Bump,
    term: &'a SerialTerm<'a>,
    cont: &'b dyn Fn(&'b Bump, &'b LinearTerm<'b>) -> R
  ) -> R {
    match term {
      SerialTerm::Null => cont(mem, _null(mem)),
      SerialTerm::Text(data) => cont(mem, _text(mem, data)),
      SerialTerm::Nest(term1) =>
        _visit_term(mem, term1, compose(mem, cont,
          mem.alloc(|mem, term2| _nest(mem, term2)))),
      SerialTerm::Pack(index, term1) =>
        _visit_term(mem, term1, compose(mem, cont,
          mem.alloc(|mem, term2| _pack(mem, *index, term2))))
    }
  }
  fn _visit_comp<'b, 'a: 'b, R>(
    mem: &'b Bump,
    comp: &'a SerialComp<'a>,
    cont: &'b dyn Fn(&'b Bump, &'b LinearComp<'b>) -> R
  ) -> R {
    match comp {
      SerialComp::Line => unreachable!("Invariant"),
      SerialComp::Comp(attr) => cont(mem, _comp(mem, *attr)),
      SerialComp::Grp(index, comp1) =>
        _visit_comp(mem, comp1, compose(mem, cont,
          mem.alloc(|mem, comp1| _grp(mem, *index, comp1)))),
      SerialComp::Seq(index, comp1) =>
        _visit_comp(mem, comp1, compose(mem, cont,
          mem.alloc(|mem, comp1| _seq(mem, *index, comp1))))
    }
  }
  _visit_serial(
    mem,
    serial,
    mem.alloc(|_mem, obj| obj),
    mem.alloc(|_mem, doc| doc),
  )
}

#[derive(Debug)]
enum FixedDoc<'a> {
  EOD,
  Break(&'a FixedObj<'a>, &'a FixedDoc<'a>)
}

#[derive(Debug)]
enum FixedObj<'a> {
  Next(&'a FixedItem<'a>, &'a FixedComp<'a>, &'a FixedObj<'a>),
  Last(&'a FixedItem<'a>)
}

#[derive(Debug)]
enum FixedItem<'a> {
  Fix(&'a FixedFix<'a>),
  Term(&'a FixedTerm<'a>)
}

#[derive(Debug)]
enum FixedTerm<'a> {
  Null,
  Text(&'a str),
  Nest(&'a FixedTerm<'a>),
  Pack(u64, &'a FixedTerm<'a>)
}

#[derive(Debug)]
enum FixedComp<'a> {
  Comp(bool),
  Grp(u64, &'a FixedComp<'a>),
  Seq(u64, &'a FixedComp<'a>)
}

#[derive(Debug)]
enum FixedFix<'a> {
  Next(&'a FixedTerm<'a>, &'a FixedComp<'a>, &'a FixedFix<'a>),
  Last(&'a FixedTerm<'a>)
}

/*
  Coalesce fixed comps
*/
fn _fixed<'b, 'a: 'b>(
  mem: &'b Bump,
  doc: &'a LinearDoc<'a>
) -> &'b FixedDoc<'b> {
  fn _eod<'a>(
    mem: &'a Bump
  ) -> &'a FixedDoc<'a> {
    mem.alloc(FixedDoc::EOD)
  }
  fn _break<'a>(
    mem: &'a Bump,
    obj: &'a FixedObj<'a>,
    doc: &'a FixedDoc<'a>
  ) -> &'a FixedDoc<'a> {
    mem.alloc(FixedDoc::Break(obj, doc))
  }
  fn _next<'a>(
    mem: &'a Bump,
    item: &'a FixedItem<'a>,
    comp: &'a FixedComp<'a>,
    obj: &'a FixedObj<'a>
  ) -> &'a FixedObj<'a> {
    mem.alloc(FixedObj::Next(item, comp, obj))
  }
  fn _last<'a>(
    mem: &'a Bump,
    item: &'a FixedItem<'a>
  ) -> &'a FixedObj<'a> {
    mem.alloc(FixedObj::Last(item))
  }
  fn _fix<'a>(
    mem: &'a Bump,
    fix: &'a FixedFix<'a>
  ) -> &'a FixedItem<'a> {
    mem.alloc(FixedItem::Fix(fix))
  }
  fn _term<'a>(
    mem: &'a Bump,
    term: &'a FixedTerm<'a>
  ) -> &'a FixedItem<'a> {
    mem.alloc(FixedItem::Term(term))
  }
  fn _null<'a>(
    mem: &'a Bump
  ) -> &'a FixedTerm<'a> {
    mem.alloc(FixedTerm::Null)
  }
  fn _text<'a>(
    mem: &'a Bump,
    data: &'a str
  ) -> &'a FixedTerm<'a> {
    mem.alloc(FixedTerm::Text(data))
  }
  fn _nest<'a>(
    mem: &'a Bump,
    term: &'a FixedTerm<'a>
  ) -> &'a FixedTerm<'a> {
    mem.alloc(FixedTerm::Nest(term))
  }
  fn _pack<'a>(
    mem: &'a Bump,
    index: u64,
    term: &'a FixedTerm<'a>
  ) -> &'a FixedTerm<'a> {
    mem.alloc(FixedTerm::Pack(index, term))
  }
  fn _comp<'a>(
    mem: &'a Bump,
    pad: bool
  ) -> &'a FixedComp<'a> {
    mem.alloc(FixedComp::Comp(pad))
  }
  fn _grp<'a>(
    mem: &'a Bump,
    index: u64,
    comp: &'a FixedComp<'a>
  ) -> &'a FixedComp<'a> {
    mem.alloc(FixedComp::Grp(index, comp))
  }
  fn _seq<'a>(
    mem: &'a Bump,
    index: u64,
    comp: &'a FixedComp<'a>
  ) -> &'a FixedComp<'a> {
    mem.alloc(FixedComp::Seq(index, comp))
  }
  fn _fix_next<'a>(
    mem: &'a Bump,
    term: &'a FixedTerm<'a>,
    comp: &'a FixedComp<'a>,
    fix: &'a FixedFix<'a>
  ) -> &'a FixedFix<'a> {
    mem.alloc(FixedFix::Next(term, comp, fix))
  }
  fn _fix_last<'a>(
    mem: &'a Bump,
    term: &'a FixedTerm<'a>
  ) -> &'a FixedFix<'a> {
    mem.alloc(FixedFix::Last(term))
  }
  fn _visit_doc<'b, 'a: 'b>(
    mem: &'b Bump,
    doc: &'a LinearDoc<'a>
  ) -> &'b FixedDoc<'b> {
    match doc {
      LinearDoc::Nil => _eod(mem),
      LinearDoc::Cons(obj, doc1) =>
        _visit_obj(mem, obj, mem.alloc(move |mem, obj1| {
        let doc2 = _visit_doc(mem, doc1);
        _break(mem, obj1, doc2)}))
    }
  }
  fn _visit_obj<'b, 'a: 'b, R>(
    mem: &'b Bump,
    obj: &'a LinearObj<'a>,
    cont: &'b dyn Fn(&'b Bump, &'b FixedObj<'b>) -> R
  ) -> R {
    match obj {
      LinearObj::Next(term, comp, obj1) =>
        _visit_term(mem, term, mem.alloc(move |mem, term1| {
        let (is_fixed, comp1) = _visit_comp(mem, comp);
        if is_fixed {
          _visit_fix(
            mem,
            obj1,
            mem.alloc(move |mem, fix| _fix_next(mem, term1, comp1, fix)),
            cont
          )
        } else {
          _visit_obj(
            mem,
            obj1,
            compose(mem, cont, mem.alloc(|mem, obj2|
              _next(mem, _term(mem, term1), comp1, obj2))
            ))
        }})),
      LinearObj::Last(term) =>
        _visit_term(mem, term, mem.alloc(|mem, term1|
        cont(mem, _last(mem, _term(mem, term1)))))
    }
  }
  fn _visit_fix<'b, 'a: 'b, R>(
    mem: &'b Bump,
    obj: &'a LinearObj<'a>,
    line: &'b dyn Fn(&'b Bump, &'b FixedFix<'b>) -> &'b FixedFix<'b>,
    cont: &'b dyn Fn(&'b Bump, &'b FixedObj<'b>) -> R
  ) -> R {
    match obj {
      LinearObj::Next(term, comp, obj1) =>
        _visit_term(mem, term, mem.alloc(move |mem, term1| {
        let (is_fixed, comp1) = _visit_comp(mem, comp);
        if is_fixed {
          _visit_fix(
            mem,
            obj1,
            compose(mem, line, mem.alloc(move |mem, fix|
              _fix_next(mem, term1, comp1, fix))),
            cont
          )
        } else {
          _visit_obj(mem, obj1, compose(mem, cont,
            mem.alloc(|mem, obj2|
              _next(
                mem,
                _fix(mem, line(mem, _fix_last(mem, term1))),
                comp1,
                obj2
              ))))
        }})),
      LinearObj::Last(term) =>
        _visit_term(mem, term, mem.alloc(|mem, term1|
        cont(mem, _last(mem, _fix(mem, line(mem, _fix_last(mem, term1)))))))
    }
  }
  fn _visit_term<'b, 'a: 'b, R>(
    mem: &'b Bump,
    term: &'a LinearTerm<'a>,
    cont: &'b dyn Fn(&'b Bump, &'b FixedTerm<'b>) -> R
  ) -> R {
    match term {
      LinearTerm::Null => cont(mem, _null(mem)),
      LinearTerm::Text(data) => cont(mem, _text(mem, data)),
      LinearTerm::Nest(term1) =>
        _visit_term(mem, term1, compose(mem, cont,
          mem.alloc(|mem, term2| _nest(mem, term2)))),
      LinearTerm::Pack(index, term1) =>
        _visit_term(mem, term1, compose(mem, cont,
          mem.alloc(|mem, term2| _pack(mem, *index, term2))))
    }
  }
  fn _visit_comp<'b, 'a: 'b>(
    mem: &'b Bump,
    comp: &'a LinearComp<'a>
  ) -> (bool, &'b FixedComp<'b>) {
    match comp {
      LinearComp::Comp(attr) => (attr.fix, _comp(mem, attr.pad)),
      LinearComp::Grp(index, comp1) => {
        let (is_fixed, comp2) = _visit_comp(mem, comp1);
        (is_fixed, _grp(mem, *index, comp2))
      }
      LinearComp::Seq(index, comp1) => {
        let (is_fixed, comp2) = _visit_comp(mem, comp1);
        (is_fixed, _seq(mem, *index, comp2))
      }
    }
  }
  _visit_doc(mem, doc)
}

#[derive(Debug, Copy, Clone)]
enum Property<T> {
  Grp(T),
  Seq(T)
}

#[derive(Debug)]
enum GraphDoc<'a> {
  EOD,
  Break(&'a List<'a, &'a GraphNode<'a>>, &'a List<'a, bool>, &'a GraphDoc<'a>)
}

#[derive(Debug)]
struct GraphNode<'a> {
  index: u64,
  term: &'a GraphTerm<'a>,
  ins_head: Cell<Option<&'a GraphEdge<'a>>>,
  ins_tail: Cell<Option<&'a GraphEdge<'a>>>,
  outs_head: Cell<Option<&'a GraphEdge<'a>>>,
  outs_tail: Cell<Option<&'a GraphEdge<'a>>>
}

#[derive(Debug)]
struct GraphEdge<'a> {
  prop: Property<()>,
  ins_next: Cell<Option<&'a GraphEdge<'a>>>,
  ins_prev: Cell<Option<&'a GraphEdge<'a>>>,
  outs_next: Cell<Option<&'a GraphEdge<'a>>>,
  outs_prev: Cell<Option<&'a GraphEdge<'a>>>,
  source: Cell<&'a GraphNode<'a>>,
  target: Cell<&'a GraphNode<'a>>
}

#[derive(Debug)]
enum GraphTerm<'a> {
  Null,
  Text(&'a str),
  Fix(&'a GraphFix<'a>),
  Nest(&'a GraphTerm<'a>),
  Pack(u64, &'a GraphTerm<'a>)
}

#[derive(Debug)]
enum GraphFix<'a> {
  Last(&'a GraphTerm<'a>),
  Next(&'a GraphTerm<'a>, &'a GraphFix<'a>, bool)
}

fn copy_graph_term<'b, 'a: 'b>(
  mem: &'b Bump,
  term: &'a GraphTerm<'a>
) -> &'b GraphTerm<'b> {
  match term {
    GraphTerm::Null => mem.alloc(GraphTerm::Null),
    GraphTerm::Text(data) => mem.alloc(GraphTerm::Text(data)),
    GraphTerm::Fix(fix) => {
      let fix1 = copy_graph_fix(mem, fix);
      mem.alloc(GraphTerm::Fix(fix1))
    },
    GraphTerm::Nest(term1) => {
      let term2 = copy_graph_term(mem, term1);
      mem.alloc(GraphTerm::Nest(term2))
    },
    GraphTerm::Pack(index, term1) => {
      let term2 = copy_graph_term(mem, term1);
      mem.alloc(GraphTerm::Pack(*index, term2))
    }
  }
}

fn copy_graph_fix<'b, 'a: 'b>(
  mem: &'b Bump,
  fix: &'a GraphFix<'a>
) -> &'b GraphFix<'b> {
  match fix {
    GraphFix::Last(term) => {
      let term1 = copy_graph_term(mem, term);
      mem.alloc(GraphFix::Last(term1))
    },
    GraphFix::Next(term, fix1, pad) => {
      let term1 = copy_graph_term(mem, term);
      let fix2 = copy_graph_fix(mem, fix1);
      mem.alloc(GraphFix::Next(term1, fix2, *pad))
    },
  }
}

fn make_node<'a>(
  mem: &'a Bump,
  index: u64,
  term: &'a GraphTerm<'a>
) -> &'a GraphNode<'a> {
  mem.alloc(GraphNode {
    index: index,
    term: term,
    ins_head: Cell::new(None),
    ins_tail: Cell::new(None),
    outs_head: Cell::new(None),
    outs_tail: Cell::new(None)
  })
}

fn make_edge<'a>(
  mem: &'a Bump,
  prop: Property<()>,
  source: &'a GraphNode<'a>,
  target: &'a GraphNode<'a>
) -> &'a GraphEdge<'a> {
  mem.alloc(GraphEdge {
    prop: prop,
    ins_next: Cell::new(None),
    ins_prev: Cell::new(None),
    outs_next: Cell::new(None),
    outs_prev: Cell::new(None),
    source: Cell::new(source),
    target: Cell::new(target)
  })
}

#[derive(Debug)]
enum RebuildDoc<'a> {
  EOD,
  Break(&'a RebuildObj<'a>, &'a RebuildDoc<'a>)
}

#[derive(Debug)]
enum RebuildObj<'a> {
  Term(&'a RebuildTerm<'a>),
  Fix(&'a RebuildFix<'a>),
  Grp(&'a RebuildObj<'a>),
  Seq(&'a RebuildObj<'a>),
  Comp(&'a RebuildObj<'a>, &'a RebuildObj<'a>, bool)
}

#[derive(Debug)]
enum RebuildFix<'a> {
  Term(&'a RebuildTerm<'a>),
  Comp(&'a RebuildFix<'a>, &'a RebuildFix<'a>, bool)
}

#[derive(Debug)]
enum RebuildTerm<'a> {
  Null,
  Text(&'a str),
  Nest(&'a RebuildTerm<'a>),
  Pack(u64, &'a RebuildTerm<'a>)
}

#[derive(Copy, Clone)]
struct RebuildCont<'a>(&'a dyn Fn(&'a Bump, &'a RebuildObj<'a>) -> &'a RebuildObj<'a>);

impl<'a> fmt::Debug for RebuildCont<'a> {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "RebuildCont(<fn>)")
  }
}

fn _structurize<'b, 'a: 'b>(
  mem: &'b Bump,
  doc: &'a FixedDoc<'a>
) -> &'b RebuildDoc<'b> {
  fn _eod<'a>(
    mem: &'a Bump
  ) -> &'a GraphDoc<'a> {
    mem.alloc(GraphDoc::EOD)
  }
  fn _break<'a>(
    mem: &'a Bump,
    nodes: &'a List<'a, &'a GraphNode<'a>>,
    pads: &'a List<'a, bool>,
    doc: &'a GraphDoc<'a>
  ) -> &'a GraphDoc<'a> {
    mem.alloc(GraphDoc::Break(nodes, pads, doc))
  }
  fn _null<'a>(
    mem: &'a Bump
  ) -> &'a GraphTerm<'a> {
    mem.alloc(GraphTerm::Null)
  }
  fn _text<'a>(
    mem: &'a Bump,
    data: &'a str
  ) -> &'a GraphTerm<'a> {
    mem.alloc(GraphTerm::Text(data))
  }
  fn _fix<'a>(
    mem: &'a Bump,
    fix: &'a GraphFix<'a>
  ) -> &'a GraphTerm<'a> {
    mem.alloc(GraphTerm::Fix(fix))
  }
  fn _nest<'a>(
    mem: &'a Bump,
    term: &'a GraphTerm<'a>
  ) -> &'a GraphTerm<'a> {
    mem.alloc(GraphTerm::Nest(term))
  }
  fn _pack<'a>(
    mem: &'a Bump,
    index: u64,
    term: &'a GraphTerm<'a>
  ) -> &'a GraphTerm<'a> {
    mem.alloc(GraphTerm::Pack(index, term))
  }
  fn _fix_last<'a>(
    mem: &'a Bump,
    term: &'a GraphTerm<'a>
  ) -> &'a GraphFix<'a> {
    mem.alloc(GraphFix::Last(term))
  }
  fn _fix_next<'a>(
    mem: &'a Bump,
    left: &'a GraphTerm<'a>,
    right: &'a GraphFix<'a>,
    pad: bool
  ) -> &'a GraphFix<'a> {
    mem.alloc(GraphFix::Next(left, right, pad))
  }
  fn _unit_grp() -> Property<()> { Property::Grp(()) }
  fn _unit_seq() -> Property<()> { Property::Seq(()) }
  fn _unary_grp(index: u64) -> Property<u64> { Property::Grp(index) }
  fn _unary_seq(index: u64) -> Property<u64> { Property::Seq(index) }
  fn _binary_grp(
    from_index: u64,
    to_index: Option<u64>
  ) -> Property<(u64, Option<u64>)> {
    Property::Grp((from_index, to_index))
  }
  fn _binary_seq(
    from_index: u64,
    to_index: Option<u64>
  ) -> Property<(u64, Option<u64>)>{
    Property::Seq((from_index, to_index))
  }
  fn _graphify<'b, 'a: 'b>(
    mem: &'b Bump,
    doc: &'a FixedDoc<'a>
  ) -> &'b GraphDoc<'b> {
    fn _lift_stack<'b, 'a: 'b>(
      mem: &'b Bump,
      comp: &'a FixedComp<'a>
    ) -> (&'b List<'b, Property<u64>>, bool) {
      match comp {
        FixedComp::Comp(pad) => (_list::nil(mem), *pad),
        FixedComp::Grp(index, comp1) => {
          let (props, pad) = _lift_stack(mem, comp1);
          (_list::cons(mem, _unary_grp(*index), props), pad)
        }
        FixedComp::Seq(index, comp1) => {
          let (props, pad) = _lift_stack(mem, comp1);
          (_list::cons(mem, _unary_seq(*index), props), pad)
        }
      }
    }
    type Graph<'a> = Map<'a, u64, Property<(u64, Option<u64>)>>;
    fn _close<'b, 'a: 'b>(
      mem: &'b Bump,
      to_node: u64,
      props: &'a Graph<'a>,
      stack: &'a List<'a, Property<u64>>
    ) -> &'b Graph<'b> {
      match stack {
        List::Nil => props,
        List::Cons(_, Property::Grp(index), stack1) =>
          match props.lookup_unsafe(&total, *index) {
            Property::Seq(_) => unreachable!("Invariant"),
            Property::Grp((from_node, _to_node)) => {
              let prop1 = _binary_grp(from_node, Some(to_node));
              let props1 = props.insert(mem, &total, *index, prop1);
              _close(mem, to_node, props1, stack1)
            }
          }
        List::Cons(_, Property::Seq(index), stack1) => {
          match props.lookup_unsafe(&total, *index) {
            Property::Grp(_) => unreachable!("Invariant"),
            Property::Seq((from_node, _to_node)) => {
              let prop1 = _binary_seq(from_node, Some(to_node));
              let props1 = props.insert(mem, &total, *index, prop1);
              _close(mem, to_node, props1, stack1)
            }
          }
        }
      }
    }
    fn _open<'b, 'a: 'b>(
      mem: &'b Bump,
      from_node: u64,
      props: &'a Graph<'a>,
      stack: &'a List<Property<u64>>
    ) -> &'b Graph<'b> {
      match stack {
        List::Nil => props,
        List::Cons(_, Property::Grp(index), stack1) => {
          let prop1 = _binary_grp(from_node, None);
          let props1 = props.insert(mem, &total, *index, prop1);
          _open(mem, from_node, props1, stack1)
        }
        List::Cons(_, Property::Seq(index), stack1) => {
          let prop1 = _binary_seq(from_node, None);
          let props1 = props.insert(mem, &total, *index, prop1);
          _open(mem, from_node, props1, stack1)
        }
      }
    }
    fn _update<'b, 'a: 'b>(
      mem: &'b Bump,
      node: u64,
      props: &'a Graph<'a>,
      scope: &'b List<'b, Property<u64>>,
      stack: &'b List<'b, Property<u64>>
    ) -> (&'b List<'b, Property<u64>>, &'b Graph<'b>) {
      match (scope, stack) {
        (_, List::Nil) => {
          let props1 = _close(mem, node, props, scope);
          (_list::nil(mem), props1)
        }
        (List::Nil, _) => {
          let props1 = _open(mem, node, props, stack);
          (stack, props1)
        }
        ( List::Cons(_, Property::Grp(left), scope1)
        , List::Cons(_, Property::Grp(right), stack1)) => {
          if left > right { unreachable!("Invariant") }
          if left == right {
            let (stack2, props1) = _update(mem, node, props, scope1, stack1);
            (_list::cons(mem, _unary_grp(*left), stack2), props1)
          } else {
            let props1 = _close(mem, node, props, scope);
            let props2 = _open(mem, node, props1, stack);
            (stack, props2)
          }
        }
        ( List::Cons(_, Property::Seq(left), scope1)
        , List::Cons(_, Property::Seq(right), stack1)) => {
          if left > right { unreachable!("Invariant") }
          if left == right {
            let (stack2, props1) = _update(mem, node, props, scope1, stack1);
            (_list::cons(mem, _unary_seq(*left), stack2), props1)
          } else {
            let props1 = _close(mem, node, props, scope);
            let props2 = _open(mem, node, props1, stack);
            (stack, props2)
          }
        }
        _ => {
          let props1 = _close(mem, node, props, scope);
          let props2 = _open(mem, node, props1, stack);
          (stack, props2)
        }
      }
    }
    fn _transpose<'a>(
      mem: &'a Bump,
      nodes: &'a List<'a, &'a GraphNode<'a>>,
      props: &'a List<'a, Property<(u64, Option<u64>)>>
    ) {
      fn _push_ins<'a>(
        edge: &'a GraphEdge<'a>,
        node: &'a GraphNode<'a>
       ) {
        match node.ins_tail.get() {
          None => {
            node.ins_head.set(Some(edge));
            node.ins_tail.set(Some(edge))
          }
          Some(tail) => {
            edge.ins_prev.set(Some(tail));
            tail.ins_next.set(Some(edge));
            node.ins_tail.set(Some(edge))
          }
        }
      }
      fn _push_outs<'a>(
        edge: &'a GraphEdge<'a>,
        node: &'a GraphNode<'a>
      ) {
        match node.outs_tail.get() {
          None => {
            node.outs_head.set(Some(edge));
            node.outs_tail.set(Some(edge))
          }
          Some(tail) => {
            edge.outs_prev.set(Some(tail));
            tail.outs_next.set(Some(edge));
            node.outs_tail.set(Some(edge))
          }
        }
      }
      match props {
        List::Nil => (),
        List::Cons(_, Property::Grp((from_index, Some(to_index))), props1) => {
          if from_index == to_index {
            _transpose(mem, nodes, props1)
          } else {
            let from_node = nodes.get_unsafe(*from_index);
            let to_node = nodes.get_unsafe(*to_index);
            let curr = make_edge(mem, _unit_grp(), from_node, to_node);
            _push_ins(curr, to_node);
            _push_outs(curr, from_node);
            _transpose(mem, nodes, props1)
          }
        }
        List::Cons(_, Property::Seq((from_index, Some(to_index))), props1) => {
          if from_index == to_index {
            _transpose(mem, nodes, props1)
          } else {
            let from_node = nodes.get_unsafe(*from_index);
            let to_node = nodes.get_unsafe(*to_index);
            let curr = make_edge(mem, _unit_seq(), from_node, to_node);
            _push_ins(curr, to_node);
            _push_outs(curr, from_node);
            _transpose(mem, nodes, props1)
          }
        }
        _ => unreachable!("Invariant")
      }
    }
    fn _visit_doc<'b, 'a: 'b>(
      mem: &'b Bump,
      doc: &'a FixedDoc<'a>
    ) -> &'b GraphDoc<'b> {
      match doc {
        FixedDoc::EOD => _eod(mem),
        FixedDoc::Break(obj, doc1) => {
          let scope = _list::nil(mem);
          let nodes = mem.alloc(|_mem, nodes| nodes);
          let pads = mem.alloc(|_mem, pads| pads);
          let props = _map::empty(mem);
          let (nodes1, pads1, props1) = _visit_obj(
            mem, obj, 0, scope, nodes, pads, props
          );
          let nodes2 = nodes1(mem, _list::nil(mem));
          let props2 = props1.values(mem).fold(
            mem,
            _list::nil(mem),
            mem.alloc(|mem, item: Property<(u64, Option<u64>)>, items|
            _list::cons(mem, item, items))
          );
          _transpose(mem, nodes2, props2);
          let doc2 = _visit_doc(mem, doc1);
          _break(mem, nodes2, pads1(mem, _list::nil(mem)), doc2)
        }
      }
    }
    fn _visit_obj<'b, 'a: 'b>(
      mem: &'b Bump,
      obj: &'a FixedObj<'a>,
      index: u64,
      scope: &'a List<'a, Property<u64>>,
      nodes: &'b dyn Fn(&'b Bump, &'b List<'b, &'b GraphNode<'b>>) -> &'b List<'b, &'b GraphNode<'b>>,
      pads: &'b dyn Fn(&'b Bump, &'b List<'b, bool>) -> &'b List<'b, bool>,
      props: &'a Graph<'a>
    ) -> (
      &'b dyn Fn(&'b Bump, &'b List<'b, &'b GraphNode<'b>>) -> &'b List<'b, &'b GraphNode<'b>>,
      &'b dyn Fn(&'b Bump, &'b List<'b, bool>) -> &'b List<'b, bool>,
      &'b Graph<'b>
    ) {
      match obj {
        FixedObj::Next(term, comp, obj1) => {
          match term {
            FixedItem::Term(term) =>
              _visit_term(mem, term, mem.alloc(move |mem, term1| {
              let nodes2 = compose(mem, nodes, mem.alloc(move |mem, nodes1|
                _list::cons(mem, make_node(mem, index, term1), nodes1)
              ));
              let (stack, pad) = _lift_stack(mem, comp);
              let pads2 = compose(mem, pads, mem.alloc(move |mem, pads1|
                _list::cons(mem, pad, pads1)
              ));
              let (scope1, props1) = _update(mem, index, props, scope, stack);
              _visit_obj(
                mem,
                obj1,
                index + 1,
                scope1,
                nodes2,
                pads2,
                props1
              )})),
            FixedItem::Fix(fix) => {
              let (fix1, scope1, props1) = _visit_fix(
                mem, fix, index, scope, props
              );
              let nodes2 = compose(mem, nodes, mem.alloc(move |mem, nodes1|
                _list::cons(mem, make_node(mem, index, _fix(mem, fix1)), nodes1)
              ));
              let (stack, pad) = _lift_stack(mem, comp);
              let pads2 = compose(mem, pads, mem.alloc(move |mem, pads1|
                _list::cons(mem, pad, pads1)
              ));
              let (scope2, props2) = _update(mem, index, props1, scope1, stack);
              _visit_obj(
                mem,
                obj1,
                index + 1,
                scope2,
                nodes2,
                pads2,
                props2
              )
            }
          }
        }
        FixedObj::Last(term) => {
          match term {
            FixedItem::Term(term) =>
              _visit_term(mem, term, mem.alloc(move |mem, term1| {
              let nodes2 = compose(mem, nodes, mem.alloc(move |mem, nodes1|
                _list::cons(mem, make_node(mem, index, term1), nodes1)
              ));
              let props1 = _close(mem, index, props, scope);
              (nodes2, pads, props1)})),
            FixedItem::Fix(fix) => {
              let (fix1, scope1, props1) = _visit_fix(mem, fix, index, scope, props);
              let nodes2 = compose(mem, nodes, mem.alloc(move |mem, nodes1|
                _list::cons(mem, make_node(mem, index, _fix(mem, fix1)), nodes1)
              ));
              let props2 = _close(mem, index, props1, scope1);
              (nodes2, pads, props2)
            }
          }
        }
      }
    }
    fn _visit_term<'b, 'a: 'b, R>(
      mem: &'b Bump,
      term: &'a FixedTerm<'a>,
      cont: &'b dyn Fn(&'b Bump, &'b GraphTerm<'b>) -> R
    ) -> R {
      match term {
        FixedTerm::Null => cont(mem, _null(mem)),
        FixedTerm::Text(data) => cont(mem, _text(mem, data)),
        FixedTerm::Nest(term1) =>
          _visit_term(mem, term1, compose(mem, cont, mem.alloc(|mem, term2|
          _nest(mem, term2)))),
        FixedTerm::Pack(index, term1) =>
          _visit_term(mem, term1, compose(mem, cont, mem.alloc(|mem, term2|
          _pack(mem, *index, term2))))
      }
    }
    fn _visit_fix<'b, 'a: 'b>(
      mem: &'b Bump,
      fix: &'a FixedFix<'a>,
      index: u64,
      scope: &'a List<'a, Property<u64>>,
      props: &'a Graph<'a>
    ) -> (
      &'b GraphFix<'b>,
      &'b List<'b, Property<u64>>,
      &'b Graph<'b>
    ) {
      match fix {
        FixedFix::Next(term, comp, fix1) =>
          _visit_term(mem, term, mem.alloc(move |mem, term1| {
          let (stack, pad) = _lift_stack(mem, comp);
          let (scope1, props1) = _update(mem, index, props, scope, stack);
          let (fix2, scope2, props2) = _visit_fix(mem, fix1, index, scope1, props1);
          (_fix_next(mem, term1, fix2, pad), scope2, props2)})),
        FixedFix::Last(term) =>
          _visit_term(mem, term, mem.alloc(move |mem, term1|
          (_fix_last(mem, term1), scope, props)))
      }
    }
    _visit_doc(mem, doc)
  }
  fn _solve<'a>(
    mem: &'a Bump,
    doc: &'a GraphDoc<'a>
  ) -> &'a GraphDoc<'a> {
    fn _move_ins<'a>(
      head: &'a GraphEdge<'a>,
      tail: &'a GraphEdge<'a>,
      edge: &'a GraphEdge<'a>
    ) {
      fn _remove_ins<'a>(ins: &'a GraphEdge<'a>) {
        let node = ins.target.get();
        node.ins_head.set(None);
        node.ins_tail.set(None)
      }
      fn _append_ins<'a>(
        head: &'a GraphEdge<'a>,
        tail: &'a GraphEdge<'a>,
        edge: &'a GraphEdge<'a>
      ) {
        fn _set_targets<'a>(
          node: &'a GraphNode<'a>,
          ins: Option<&'a GraphEdge<'a>>
        ) {
          match ins {
            None => (),
            Some(edge) => {
              edge.target.set(node);
              _set_targets(node, edge.ins_next.get())
            }
          }
        }
        let node = edge.target.get();
        _set_targets(node, Some(head));
        match edge.ins_next.get() {
          None => {
            edge.ins_next.set(Some(head));
            head.ins_prev.set(Some(edge));
            node.ins_tail.set(Some(tail))
          }
          Some(next) => {
            tail.ins_next.set(Some(next));
            next.ins_prev.set(Some(tail));
            edge.ins_next.set(Some(head));
            head.ins_prev.set(Some(edge))
          }
        }
      }
      _remove_ins(head);
      _append_ins(head, tail, edge)
    }
    fn _move_out<'a>(
      curr: &'a GraphEdge<'a>,
      edge: &'a GraphEdge<'a>
    ) {
      fn _remove_out<'a>(curr: &'a GraphEdge<'a>) {
        let node = curr.source.get();
        match (curr.outs_prev.get(), curr.outs_next.get()) {
          (None, None) => {
            node.outs_head.set(None);
            node.outs_tail.set(None)
          }
          (Some(prev), None) => {
            curr.outs_prev.set(None);
            prev.outs_next.set(None);
            node.outs_tail.set(Some(prev))
          }
          (None, Some(next)) => {
            curr.outs_next.set(None);
            next.outs_prev.set(None);
            node.outs_head.set(Some(next))
          }
          (Some(prev), Some(next)) => {
            curr.outs_prev.set(None);
            curr.outs_next.set(None);
            prev.outs_next.set(Some(next));
            next.outs_prev.set(Some(prev))
          }
        }
      }
      fn _prepend_out<'a>(
        curr: &'a GraphEdge<'a>,
        edge: &'a GraphEdge<'a>
      ) {
        let node = edge.source.get();
        curr.source.set(node);
        match edge.outs_prev.get() {
          None => {
            curr.outs_next.set(Some(edge));
            edge.outs_prev.set(Some(curr));
            node.outs_head.set(Some(curr))
          }
          Some(prev) => {
            prev.outs_next.set(Some(curr));
            curr.outs_prev.set(Some(prev));
            curr.outs_next.set(Some(edge));
            edge.outs_prev.set(Some(curr));
          }
        }
      }
      _remove_out(curr);
      _prepend_out(curr, edge)
    }
    fn _resolve<'a, R>(
      mem: &'a Bump,
      edge: &'a GraphEdge<'a>,
      outs: &'a GraphEdge<'a>,
      none: &'a dyn Fn(&'a Bump) -> R,
      some: &'a dyn Fn(&'a Bump, &'a GraphEdge<'a>) -> R
    ) -> R {
      fn _visit<'a, R>(
        mem: &'a Bump,
        maybe_curr: Option<&'a GraphEdge<'a>>,
        edge: &'a GraphEdge<'a>,
        none: &'a dyn Fn(&'a Bump) -> R,
        some: &'a dyn Fn(&'a Bump, &'a GraphEdge<'a>) -> R
      ) -> R{
        match maybe_curr {
          None => none(mem),
          Some(curr) =>
            match curr.prop {
            | Property::Grp(()) => some(mem, curr),
            | Property::Seq(()) => {
              let curr1 = curr.outs_next.get();
              _move_out(curr, edge);
              _visit(mem, curr1, curr, none, some)
            }
          }
        }
      }
      _visit(mem, Some(outs), edge, none, some)
    }
    fn _leftmost<'a>(
      mem: &'a Bump,
      head: &'a GraphEdge<'a>
    ) -> &'a GraphEdge<'a> {
      fn _visit<'a>(
        mem: &'a Bump,
        curr: &'a GraphEdge<'a>,
        index: u64,
        result: &'a GraphEdge<'a>
      ) -> &'a GraphEdge<'a> {
        match curr.ins_next.get() {
          None => result,
          Some(next) => {
            let index1 = next.source.get().index;
            if index1 < index {
              _visit(mem, next, index1, next)
            } else {
              _visit(mem, next, index, result)
            }
          }
        }
      }
      _visit(mem, head, head.source.get().index, head)
    }
    fn _visit_doc<'a>(
      mem: &'a Bump,
      doc: &'a GraphDoc<'a>
    ) -> &'a GraphDoc<'a> {
      match doc {
        GraphDoc::EOD => _eod(mem),
        GraphDoc::Break(nodes, pads, doc1) => {
          let count = nodes.length();
          _visit_node(mem, count, 0, nodes);
          let doc2 = _visit_doc(mem, doc1);
          _break(mem, nodes, pads, doc2)
        }
      }
    }
    fn _visit_node<'a>(
      mem: &'a Bump,
      count: u64,
      index: u64,
      nodes: &'a List<'a, &'a GraphNode<'a>>
    ) {
      if count == index { return }
      let node = nodes.get_unsafe(index);
      match (
        (node.ins_head.get(), node.ins_tail.get()),
        (node.outs_head.get(), node.outs_tail.get())
      ) {
        ( (Some(ins_head), Some(ins_tail))
        , (Some(outs_head), Some(_outs_tail))) => {
          let ins_first = _leftmost(mem, ins_head);
          _resolve(mem, ins_first, outs_head,
            mem.alloc(move |mem| _visit_node(mem, count, index + 1, nodes)),
            mem.alloc(move |mem, outs_head1| {
              _move_ins(ins_head, ins_tail, outs_head1);
              _visit_node(mem, count, index + 1, nodes)
            }))
        }
        ((Some(_), None), _) | ((None, Some(_)), _)
        | (_, (Some(_), None)) | (_, (None, Some(_))) =>
          unreachable!("Invariant"),
        (_, _) => _visit_node(mem, count, index + 1, nodes)
      }
    }
    _visit_doc(mem, doc)
  }
  fn _rebuild<'b, 'a: 'b>(
    mem: &'b Bump,
    doc: &'a GraphDoc<'a>
  ) -> &'b RebuildDoc<'b> {
    fn _eod<'a>(
      mem: &'a Bump
    ) -> &'a RebuildDoc<'a> {
      mem.alloc(RebuildDoc::EOD)
    }
    fn _break<'a>(
      mem: &'a Bump,
      obj: &'a RebuildObj<'a>,
      doc: &'a RebuildDoc<'a>
    ) -> &'a RebuildDoc<'a> {
      mem.alloc(RebuildDoc::Break(obj, doc))
    }
    fn _term<'a>(
      mem: &'a Bump,
      term: &'a RebuildTerm<'a>
    ) -> &'a RebuildObj<'a> {
      mem.alloc(RebuildObj::Term(term))
    }
    fn _fix<'a>(
      mem: &'a Bump,
      fix: &'a RebuildFix<'a>
    ) -> &'a RebuildObj<'a> {
      mem.alloc(RebuildObj::Fix(fix))
    }
    fn _grp<'a>(
      mem: &'a Bump,
      obj: &'a RebuildObj<'a>
    ) -> &'a RebuildObj<'a> {
      mem.alloc(RebuildObj::Grp(obj))
    }
    fn _seq<'a>(
      mem: &'a Bump,
      obj: &'a RebuildObj<'a>
    ) -> &'a RebuildObj<'a> {
      mem.alloc(RebuildObj::Seq(obj))
    }
    fn _comp<'a>(
      mem: &'a Bump,
      left: &'a RebuildObj<'a>,
      right: &'a RebuildObj<'a>,
      pad: bool
    ) -> &'a RebuildObj<'a> {
      mem.alloc(RebuildObj::Comp(left, right, pad))
    }
    fn _fix_term<'a>(
      mem: &'a Bump,
      term: &'a RebuildTerm<'a>
    ) -> &'a RebuildFix<'a> {
      mem.alloc(RebuildFix::Term(term))
    }
    fn _fix_comp<'a>(
      mem: &'a Bump,
      left: &'a RebuildFix<'a>,
      right: &'a RebuildFix<'a>,
      pad: bool
    ) -> &'a RebuildFix<'a> {
      mem.alloc(RebuildFix::Comp(left, right, pad))
    }
    fn _null<'a>(
      mem: &'a Bump,
    ) -> &'a RebuildTerm<'a> {
      mem.alloc(RebuildTerm::Null)
    }
    fn _text<'a>(
      mem: &'a Bump,
      data: &'a str
    ) -> &'a RebuildTerm<'a> {
      mem.alloc(RebuildTerm::Text(data))
    }
    fn _nest<'a>(
      mem: &'a Bump,
      term: &'a RebuildTerm<'a>
    ) -> &'a RebuildTerm<'a> {
      mem.alloc(RebuildTerm::Nest(term))
    }
    fn _pack<'a>(
      mem: &'a Bump,
      index: u64,
      term: &'a RebuildTerm<'a>
    ) -> &'a RebuildTerm<'a> {
      mem.alloc(RebuildTerm::Pack(index, term))
    }
    fn __comp<'a>(
      mem: &'a Bump,
      left: &'a RebuildObj<'a>,
      pad: bool,
      right: &'a RebuildObj<'a>
    ) -> &'a RebuildObj<'a> {
      _comp(mem, left, right, pad)
    }
    fn _topology<'b, 'a: 'b>(
      mem: &'b Bump,
      nodes: &'a List<'a, &'a GraphNode<'a>>
    ) -> (
      &'b List<'b, &'b GraphTerm<'b>>,
      &'b List<'b, u64>,
      &'b List<'b, &'b List<'b, Property<()>>>
    ) {
      fn _num_ins<'a>(
        node: &'a GraphNode<'a>
      ) -> u64 {
        fn _visit<'a>(
          maybe_edge: Option<&'a GraphEdge<'a>>,
          num: u64
        ) -> u64 {
          match maybe_edge {
            None => num,
            Some(edge) => _visit(edge.ins_next.get(), num + 1)
          }
        }
        _visit(node.ins_head.get(), 0)
      }
      fn _prop_outs<'b, 'a: 'b>(
        mem: &'b Bump,
        node: &'a GraphNode<'a>
      ) -> &'b List<'b, Property<()>> {
        fn _visit<'b, 'a: 'b>(
          mem: &'b Bump,
          maybe_edge: Option<&'a GraphEdge<'a>>,
          props: &'b dyn Fn(&'b Bump, &'b List<'b, Property<()>>) -> &'b List<'b, Property<()>>
        ) -> &'b List<'b, Property<()>> {
          match maybe_edge {
            None => props(mem, _list::nil(mem)),
            Some(edge) =>
              _visit(mem, edge.outs_next.get(),
                compose(mem, props, mem.alloc(|mem, props1|
                  _list::cons(mem, edge.prop, props1))))
          }
        }
        _visit(
          mem,
          node.outs_head.get(),
          mem.alloc(|_mem, props| props)
        )
      }
      fn _visit<'b, 'a: 'b>(
        mem: &'b Bump,
        nodes: &'a List<'a, &'a GraphNode<'a>>,
        index: u64,
        terms: &'b dyn Fn(&'b Bump, &'b List<'b, &'b GraphTerm<'b>>) -> &'b List<'b, &'b GraphTerm<'b>>,
        ins: &'b dyn Fn(&'b Bump, &'b List<'b, u64>) -> &'b List<'b, u64>,
        outs: &'b dyn Fn(&'b Bump, &'b List<'b, &'b List<'b, Property<()>>>) -> &'b List<'b, &'b List<'b, Property<()>>>
      ) -> (
        &'b List<'b, &'b GraphTerm<'b>>,
        &'b List<'b, u64>,
        &'b List<'b, &'b List<'b, Property<()>>>
      ) {
        if index == nodes.length() {
          (
            terms(mem, _list::nil(mem)),
            ins(mem, _list::nil(mem)),
            outs(mem, _list::nil(mem))
          )
        } else {
          let node = nodes.get_unsafe(index);
          let term1 = copy_graph_term(mem, node.term);
          let num_ins = _num_ins(node);
          let prop_outs = _prop_outs(mem, node);
          _visit(mem, nodes, index + 1,
            compose(mem, terms, mem.alloc(move |mem, term2|
              _list::cons(mem, term1, term2))),
            compose(mem, ins, mem.alloc(move |mem, ins1|
              _list::cons(mem, num_ins, ins1))),
            compose(mem, outs, mem.alloc(move |mem, outs1|
              _list::cons(mem, prop_outs, outs1)))
          )
        }
      }
      _visit(
        mem,
        nodes,
        0,
        mem.alloc(|_mem, terms| terms),
        mem.alloc(|_mem, ins| ins),
        mem.alloc(|_mem, outs| outs)
      )
    }
    fn _open<'a>(
      mem: &'a Bump,
      props: &'a List<'a, Property<()>>,
      stack: &'a List<'a, RebuildCont<'a>>,
      partial: &'a dyn Fn(&'a Bump, &'a RebuildObj<'a>) -> &'a RebuildObj<'a>
    ) -> &'a List<'a, RebuildCont<'a>> {
      fn _visit<'a>(
        mem: &'a Bump,
        props: &'a List<'a, Property<()>>,
        stack: &'a List<'a, RebuildCont<'a>>
      ) -> &'a List<'a, RebuildCont<'a>> {
        match props {
          List::Nil => stack,
          List::Cons(_, Property::Grp(()), props1) =>
            _visit(
              mem,
              props1,
              _list::cons(mem, RebuildCont(mem.alloc(|mem, obj| _grp(mem, obj))), stack)
            ),
          List::Cons(_, Property::Seq(()), props1) =>
            _visit(
              mem,
              props1,
              _list::cons(mem, RebuildCont(mem.alloc(|mem, obj| _seq(mem, obj))), stack)
            )
        }
      }
      match stack {
        List::Cons(_, top, stack1) =>
          _visit(
            mem,
            props,
            _list::cons(
              mem,
              RebuildCont(mem.alloc(|mem, obj| top.0(mem, partial(mem, obj)))),
              stack1
            )
          ),
        _ => unreachable!("Invariant")
      }
    }
    fn _close<'a>(
      mem: &'a Bump,
      count: u64,
      stack: &'a List<'a, RebuildCont<'a>>,
      term: &'a RebuildObj<'a>
    ) -> (
      &'a List<'a, RebuildCont<'a>>,
      &'a RebuildObj<'a>
    ) {
      fn _visit<'a>(
        mem: &'a Bump,
        count: u64,
        stack: &'a List<'a, RebuildCont<'a>>,
        result: &'a RebuildObj<'a>
      ) -> (
        &'a List<'a, RebuildCont<'a>>,
        &'a RebuildObj<'a>
      ) {
        if count == 0 { (stack, result) } else {
          match stack {
            List::Cons(_, top, stack1) =>
              _visit(mem, count - 1, stack1, top.0(mem, result)),
            _ => unreachable!("Invariant")
          }
        }
      }
      _visit(mem, count, stack, term)
    }
    fn _final<'a>(
      mem: &'a Bump,
      stack: &'a List<'a, RebuildCont<'a>>,
      term: &'a RebuildObj<'a>
    ) -> &'a RebuildObj<'a> {
      match stack {
        List::Cons(_, last, List::Nil) => last.0(mem, term),
        _ => unreachable!("Invariant")
      }
    }
    fn _visit_doc<'b, 'a: 'b>(
      mem: &'b Bump,
      doc: &'a GraphDoc<'a>
    ) -> &'b RebuildDoc<'b> {
      match doc {
        GraphDoc::EOD => _eod(mem),
        GraphDoc::Break(nodes, pads, doc1) => {
          let (terms, ins, outs) = _topology(mem, nodes);
          let stack: &'b List<'b, RebuildCont<'b>> = _list::cons(
            mem,
            RebuildCont(mem.alloc(|_mem, obj| obj)),
            _list::nil(mem)
          );
          let partial = mem.alloc(|_mem, obj| obj);
          let obj = _visit_line(mem, terms, pads, ins, outs, stack, partial);
          let doc2 = _visit_doc(mem, doc1);
          _break(mem, obj, doc2)
        }
      }
    }
    fn _visit_line<'a>(
      mem: &'a Bump,
      terms: &'a List<'a, &'a GraphTerm<'a>>,
      pads: &'a List<'a, bool>,
      ins: &'a List<'a, u64>,
      outs: &'a List<'a, &'a List<'a, Property<()>>>,
      stack: &'a List<'a, RebuildCont<'a>>,
      partial: &'a dyn Fn(&'a Bump, &'a RebuildObj<'a>) -> &'a RebuildObj<'a>
    ) -> &'a RebuildObj<'a> {
      match (terms, pads) {
        ( List::Cons(_, GraphTerm::Fix(fix), List::Nil)
        , List::Nil) =>
          _visit_fix(mem, fix, mem.alloc(move |mem, fix1|
          match (ins, outs) {
            ( List::Cons(_, 0, List::Nil)
            , List::Cons(_, List::Nil, List::Nil)) =>
              _final(mem, stack, partial(mem, _fix(mem, fix1))),
            ( List::Cons(_, in_props, List::Nil)
            , List::Cons(_, List::Nil, List::Nil)) => {
              let (stack1, fix2) = _close(
                mem, *in_props, stack, partial(mem, _fix(mem, fix1))
              );
              _final(mem, stack1, fix2)
            }
            (_, _) => unreachable!("Invariant")
          })),
        ( List::Cons(_, term, List::Nil)
        , List::Nil) =>
          _visit_term(mem, term, mem.alloc(move |mem, term1|
          match (ins, outs) {
            ( List::Cons(_, 0, List::Nil)
            , List::Cons(_, List::Nil, List::Nil)) =>
              _final(mem, stack, partial(mem, _term(mem, term1))),
            ( List::Cons(_, in_props, List::Nil)
            , List::Cons(_, List::Nil, List::Nil)) => {
              let (stack1, term2) = _close(
                mem, *in_props, stack, partial(mem, _term(mem, term1))
              );
              _final(mem, stack1, term2)
            }
            (_, _) => unreachable!("Invariant")
          })),
        ( List::Cons(_, GraphTerm::Fix(fix), terms1)
        , List::Cons(_, pad, pads1)) =>
          _visit_fix(mem, fix, mem.alloc(move |mem, fix1|
          match (ins, outs) {
            ( List::Cons(_, 0, ins1)
            , List::Cons(_, List::Nil, outs1)) => {
              let partial1 = compose(mem, partial, mem.alloc(move |mem, obj|
                __comp(mem, _fix(mem, fix1), *pad, obj)
              ));
              _visit_line(mem, terms1, pads1, ins1, outs1, stack, partial1)
            }
            ( List::Cons(_, in_props, ins1)
            , List::Cons(_, List::Nil, outs1)) => {
              let (stack1, fix2) = _close(
                mem, *in_props, stack, partial(mem, _fix(mem, fix1))
              );
              let partial1 = mem.alloc(move |mem, obj|
                __comp(mem, fix2, *pad, obj)
              );
              _visit_line(mem, terms1, pads1, ins1, outs1, stack1, partial1)
            }
            ( List::Cons(_, 0, ins1)
            , List::Cons(_, out_props, outs1)) => {
              let stack1 = _open(mem, out_props, stack, partial);
              let partial1 = mem.alloc(|mem, obj|
                __comp(mem, _fix(mem, fix1), *pad, obj)
              );
              _visit_line(mem, terms1, pads1, ins1, outs1, stack1, partial1)
            }
            (_, _) => unreachable!("Invariant")
          })),
        ( List::Cons(_, term, terms1)
        , List::Cons(_, pad, pads1)) =>
          _visit_term(mem, term, mem.alloc(move |mem, term1|
          match (ins, outs) {
            ( List::Cons(_, 0, ins1)
            , List::Cons(_, List::Nil, outs1)) => {
              let partial1 = compose(mem, partial, mem.alloc(move |mem, obj|
                __comp(mem, _term(mem, term1), *pad, obj)
              ));
              _visit_line(mem, terms1, pads1, ins1, outs1, stack, partial1)
            }
            ( List::Cons(_, in_props, ins1)
            , List::Cons(_, List::Nil, outs1)) => {
              let (stack1, term2) = _close(
                mem, *in_props, stack, partial(mem, _term(mem, term1))
              );
              let partial1 = mem.alloc(move |mem, obj|
                __comp(mem, term2, *pad, obj)
              );
              _visit_line(mem, terms1, pads1, ins1, outs1, stack1, partial1)
            }
            ( List::Cons(_, 0, ins1)
            , List::Cons(_, out_props, outs1)) => {
              let stack1 = _open(mem, out_props, stack, partial);
              let partial1 = mem.alloc(|mem, obj|
                __comp(mem, _term(mem, term1), *pad, obj)
              );
              _visit_line(mem, terms1, pads1, ins1, outs1, stack1, partial1)
            }
            (_, _) => unreachable!("Invariant")
          })),
        (_, _) => unreachable!("Invariant")
      }
    }
    fn _visit_term<'b, 'a: 'b, R>(
      mem: &'b Bump,
      term: &'a GraphTerm<'a>,
      cont: &'b dyn Fn(&'b Bump, &'b RebuildTerm<'b>) -> R
    ) -> R {
      match term {
        GraphTerm::Null =>
          cont(mem, _null(mem)),
        GraphTerm::Text(data) =>
          cont(mem, _text(mem, data)),
        GraphTerm::Nest(term1) =>
          _visit_term(mem, term1, compose(mem, cont, mem.alloc(|mem, term2|
          _nest(mem, term2)))),
        GraphTerm::Pack(index, term1) =>
          _visit_term(mem, term1, compose(mem, cont, mem.alloc(|mem, term2|
          _pack(mem, *index, term2)))),
        GraphTerm::Fix(_fix) =>
          unreachable!("Invariant")
      }
    }
    fn _visit_fix<'b, 'a: 'b, R>(
      mem: &'b Bump,
      fix: &'a GraphFix<'a>,
      cont: &'b dyn Fn(&'b Bump, &'b RebuildFix<'b>) -> R
    ) -> R {
      match fix {
        GraphFix::Last(term) =>
          _visit_term(mem, term, compose(mem, cont, mem.alloc(|mem, term1|
          _fix_term(mem, term1)))),
        GraphFix::Next(term, fix1, pad) =>
          _visit_term(mem, term, mem.alloc(move |mem, term1|
          _visit_fix(mem, fix1, mem.alloc(move |mem, fix2|
          cont(mem, _fix_comp(mem, _fix_term(mem, term1), fix2, *pad))))))
      }
    }
    _visit_doc(mem, doc)
  }
  let doc1 = _graphify(mem, doc);
  let doc2 = _solve(mem, doc1);
  _rebuild(mem, doc2)
}

#[derive(Debug)]
enum DenullDoc<'a> {
  EOD,
  Line(&'a DenullObj<'a>),
  Empty(&'a DenullDoc<'a>),
  Break(&'a DenullObj<'a>, &'a DenullDoc<'a>)
}

#[derive(Debug)]
enum DenullObj<'a> {
  Term(&'a DenullTerm<'a>),
  Fix(&'a DenullFix<'a>),
  Grp(&'a DenullObj<'a>),
  Seq(&'a DenullObj<'a>),
  Comp(&'a DenullObj<'a>, &'a DenullObj<'a>, bool)
}

#[derive(Debug)]
enum DenullFix<'a> {
  Term(&'a DenullTerm<'a>),
  Comp(&'a DenullFix<'a>, &'a DenullFix<'a>, bool)
}

#[derive(Debug)]
enum DenullTerm<'a> {
  Text(&'a str),
  Nest(&'a DenullTerm<'a>),
  Pack(u64, &'a DenullTerm<'a>)
}

/*
  Remove null identities
*/
fn _denull<'b, 'a: 'b>(
  mem: &'b Bump,
  doc: &'a RebuildDoc<'a>
) -> &'b DenullDoc<'b> {
  fn _eod<'a>(
    mem: &'a Bump
  ) -> &'a DenullDoc<'a> {
    mem.alloc(DenullDoc::EOD)
  }
  fn _line<'a>(
    mem: &'a Bump,
    obj: &'a DenullObj<'a>
  ) -> &'a DenullDoc<'a> {
    mem.alloc(DenullDoc::Line(obj))
  }
  fn _empty<'a>(
    mem: &'a Bump,
    doc: &'a DenullDoc<'a>
  ) -> &'a DenullDoc<'a> {
    mem.alloc(DenullDoc::Empty(doc))
  }
  fn _break<'a>(
    mem: &'a Bump,
    obj: &'a DenullObj<'a>,
    doc: &'a DenullDoc<'a>
  ) -> &'a DenullDoc<'a> {
    mem.alloc(DenullDoc::Break(obj, doc))
  }
  fn _term<'a>(
    mem: &'a Bump,
    term: &'a DenullTerm<'a>
  ) -> &'a DenullObj<'a> {
    mem.alloc(DenullObj::Term(term))
  }
  fn _fix<'a>(
    mem: &'a Bump,
    fix: &'a DenullFix<'a>
  ) -> &'a DenullObj<'a> {
    mem.alloc(DenullObj::Fix(fix))
  }
  fn _grp<'a>(
    mem: &'a Bump,
    obj: &'a DenullObj<'a>
  ) -> &'a DenullObj<'a> {
    mem.alloc(DenullObj::Grp(obj))
  }
  fn _seq<'a>(
    mem: &'a Bump,
    obj: &'a DenullObj<'a>
  ) -> &'a DenullObj<'a> {
    mem.alloc(DenullObj::Seq(obj))
  }
  fn _comp<'a>(
    mem: &'a Bump,
    left: &'a DenullObj<'a>,
    right: &'a DenullObj<'a>,
    pad: bool
  ) -> &'a DenullObj<'a> {
    mem.alloc(DenullObj::Comp(left, right, pad))
  }
  fn _fix_term<'a>(
    mem: &'a Bump,
    term: &'a DenullTerm<'a>
  ) -> &'a DenullFix<'a> {
    mem.alloc(DenullFix::Term(term))
  }
  fn _fix_comp<'a>(
    mem: &'a Bump,
    left: &'a DenullFix<'a>,
    right: &'a DenullFix<'a>,
    pad: bool
  ) -> &'a DenullFix<'a> {
    mem.alloc(DenullFix::Comp(left, right, pad))
  }
  fn _text<'a>(
    mem: &'a Bump,
    data: &'a str
  ) -> &'a DenullTerm<'a> {
    mem.alloc(DenullTerm::Text(data))
  }
  fn _nest<'a>(
    mem: &'a Bump,
    term: &'a DenullTerm<'a>
  ) -> &'a DenullTerm<'a> {
    mem.alloc(DenullTerm::Nest(term))
  }
  fn _pack<'a>(
    mem: &'a Bump,
    index: u64,
    term: &'a DenullTerm<'a>
  ) -> &'a DenullTerm<'a> {
    mem.alloc(DenullTerm::Pack(index, term))
  }
  fn _visit_doc<'b, 'a: 'b, R>(
    mem: &'b Bump,
    doc: &'a RebuildDoc<'a>,
    none: &'b dyn Fn(&'b Bump) -> R,
    some: &'b dyn Fn(&'b Bump, &'b DenullDoc<'b>) -> R
  ) -> R {
    match doc {
      RebuildDoc::EOD => none(mem),
      RebuildDoc::Break(obj, doc1) =>
        _visit_obj(mem, obj,
          mem.alloc(|mem|
            _visit_doc(mem, doc1,
              mem.alloc(|mem| some(mem, _eod(mem))),
              mem.alloc(|mem, doc2| some(mem, _empty(mem, doc2))))),
          mem.alloc(move |mem, obj1|
            _visit_doc(mem, doc1,
              mem.alloc(move |mem| some(mem, _line(mem, obj1))),
              mem.alloc(|mem, doc2| some(mem, _break(mem, obj1, doc2))))),
          mem.alloc(move |mem, _pad, obj1|
            _visit_doc(mem, doc1,
              mem.alloc(move |mem| some(mem, _line(mem, obj1))),
              mem.alloc(|mem, doc2| some(mem, _break(mem, obj1, doc2))))))
    }
  }
  fn _visit_obj<'b, 'a: 'b, R>(
    mem: &'b Bump,
    obj: &'a RebuildObj<'a>,
    last_none: &'b dyn Fn(&'b Bump) -> R,
    last_some: &'b dyn Fn(&'b Bump, &'b DenullObj<'b>) -> R,
    next_none: &'b dyn Fn(&'b Bump, bool, &'b DenullObj<'b>) -> R
  ) -> R {
    match obj {
      RebuildObj::Term(term) =>
        _visit_term(mem, term, last_none, compose(mem, last_some,
          mem.alloc(|mem, term1| _term(mem, term1)))),
      RebuildObj::Fix(fix) =>
        _visit_fix(mem, fix, last_none,
          compose(mem, last_some, mem.alloc(|mem, fix1| _fix(mem, fix1))),
          mem.alloc(|mem, _comp, fix1| last_some(mem, _fix(mem, fix1)))),
      RebuildObj::Grp(obj1) =>
        _visit_obj(mem, obj1,
          last_none,
          compose(mem, last_some, mem.alloc(|mem, obj2| _grp(mem, obj2))),
          mem.alloc(|mem, _pad, obj2|
            last_some(mem, _grp(mem, obj2)))),
      RebuildObj::Seq(obj1) =>
        _visit_obj(mem, obj1,
          last_none,
          compose(mem, last_some, mem.alloc(|mem, obj2| _seq(mem, obj2))),
          mem.alloc(|mem, _pad, obj2|
            last_some(mem, _seq(mem, obj2)))),
      RebuildObj::Comp(left, right, l_pad) =>
        _visit_obj(mem, left,
          mem.alloc(|mem|
            _visit_obj(mem, right,
              last_none,
              mem.alloc(|mem, right1| next_none(mem, *l_pad, right1)),
              mem.alloc(|mem, r_pad, right1| {
                let pad = *l_pad || r_pad;
                next_none(mem, pad, right1)}))),
          mem.alloc(move |mem, left1|
            _visit_obj(mem, right,
              mem.alloc(move |mem| last_some(mem, left1)),
              mem.alloc(|mem, right1| last_some(mem, _comp(mem, left1, right1, *l_pad))),
              mem.alloc(|mem, r_pad, right1| {
                let pad = *l_pad || r_pad;
                last_some(mem, _comp(mem, left1, right1, pad))}))),
          mem.alloc(|_mem, _pad, _left1| unreachable!("Invariant")))
    }
  }
  fn _visit_fix<'b, 'a: 'b, R>(
    mem: &'b Bump,
    fix: &'a RebuildFix<'a>,
    last_none: &'b dyn Fn(&'b Bump) -> R,
    last_some: &'b dyn Fn(&'b Bump, &'b DenullFix<'b>) -> R,
    next_none: &'b dyn Fn(&'b Bump, bool, &'b DenullFix<'b>) -> R
  ) -> R {
    match fix {
      RebuildFix::Term(term) =>
        _visit_term(mem, term, last_none, compose(mem, last_some,
          mem.alloc(|mem, term1| _fix_term(mem, term1)))),
      RebuildFix::Comp(left, right, l_pad) =>
        _visit_fix(mem, left,
          mem.alloc(|mem|
            _visit_fix(mem, right,
              last_none,
              mem.alloc(|mem, right1| next_none(mem, *l_pad, right1)),
              mem.alloc(|mem, r_pad, right1| {
                let pad = *l_pad || r_pad;
                next_none(mem, pad, right1)}))),
          mem.alloc(move |mem, left1|
            _visit_fix(mem, right,
              mem.alloc(move |mem| last_some(mem, left1)),
              mem.alloc(|mem, right1| last_some(mem, _fix_comp(mem, left1, right1, *l_pad))),
              mem.alloc(|mem, r_pad, right1| {
                let pad = *l_pad || r_pad;
                last_some(mem, _fix_comp(mem, left1, right1, pad))}))),
          mem.alloc(|_mem, _pad, _left1| unreachable!("Invariant")))
    }
  }
  fn _visit_term<'b, 'a: 'b, R>(
    mem: &'b Bump,
    term: &'a RebuildTerm<'a>,
    none: &'b dyn Fn(&'b Bump) -> R,
    some: &'b dyn Fn(&'b Bump, &'b DenullTerm<'b>) -> R
  ) -> R {
    match term {
      RebuildTerm::Null => none(mem),
      RebuildTerm::Text(data) =>
        if data.len() == 0 {
          none(mem)
        } else {
          some(mem, _text(mem, data))
        },
      RebuildTerm::Nest(term1) =>
        _visit_term(mem, term1, none, compose(mem, some,
          mem.alloc(|mem, term2| _nest(mem, term2)))),
      RebuildTerm::Pack(index, term1) =>
        _visit_term(mem, term1, none, compose(mem, some,
          mem.alloc(|mem, term2| _pack(mem, *index, term2))))
    }
  }
  _visit_doc(
    mem,
    doc,
    mem.alloc(|mem| _eod(mem)),
    mem.alloc(|_mem, doc1| doc1)
  )
}

#[derive(Debug, Copy, Clone)]
enum Count {
  Zero,
  One,
  Many
}

/*
  Remove grp and seq identities
*/
fn _identities<'b, 'a: 'b>(
  mem: &'b Bump,
  doc: &'a DenullDoc<'a>
) -> &'b DenullDoc<'b> {
  fn _eod<'a>(
    mem: &'a Bump
  ) -> &'a DenullDoc<'a> {
    mem.alloc(DenullDoc::EOD)
  }
  fn _empty<'a>(
    mem: &'a Bump,
    doc: &'a DenullDoc<'a>
  ) -> &'a DenullDoc<'a> {
    mem.alloc(DenullDoc::Empty(doc))
  }
  fn _break<'a>(
    mem: &'a Bump,
    obj: &'a DenullObj<'a>,
    doc: &'a DenullDoc<'a>
  ) -> &'a DenullDoc<'a> {
    mem.alloc(DenullDoc::Break(obj, doc))
  }
  fn _line<'a>(
    mem: &'a Bump,
    obj: &'a DenullObj<'a>
  ) -> &'a DenullDoc<'a> {
    mem.alloc(DenullDoc::Line(obj))
  }
  fn _term<'a>(
    mem: &'a Bump,
    term: &'a DenullTerm<'a>
  ) -> &'a DenullObj<'a> {
    mem.alloc(DenullObj::Term(term))
  }
  fn _fix<'a>(
    mem: &'a Bump,
    fix: &'a DenullFix<'a>
  ) -> &'a DenullObj<'a> {
    mem.alloc(DenullObj::Fix(fix))
  }
  fn _grp<'a>(
    mem: &'a Bump,
    obj: &'a DenullObj<'a>
  ) -> &'a DenullObj<'a> {
    mem.alloc(DenullObj::Grp(obj))
  }
  fn _seq<'a>(
    mem: &'a Bump,
    obj: &'a DenullObj<'a>
  ) -> &'a DenullObj<'a> {
    mem.alloc(DenullObj::Seq(obj))
  }
  fn _comp<'a>(
    mem: &'a Bump,
    left: &'a DenullObj<'a>,
    right: &'a DenullObj<'a>,
    pad: bool
  ) -> &'a DenullObj<'a> {
    mem.alloc(DenullObj::Comp(left, right, pad))
  }
  fn _add(left: Count, right: Count) -> Count {
    match (left, right) {
      (Count::Zero, _) => right,
      (_, Count::Zero) => left,
      (Count::Many, _) | (_, Count::Many) |
      (Count::One, Count::One) => Count::Many
    }
  }
  fn _elim_seqs<'b, 'a: 'b>(
    mem: &'b Bump,
    doc: &'a DenullDoc<'a>
  ) -> &'b DenullDoc<'b> {
    fn _visit_doc<'b, 'a: 'b>(
      mem: &'b Bump,
      doc: &'a DenullDoc<'a>
    ) -> &'b DenullDoc<'b> {
      match doc {
        DenullDoc::EOD => _eod(mem),
        DenullDoc::Empty(doc1) => {
          let doc2 = _visit_doc(mem, doc1);
          _empty(mem, doc2)
        }
        DenullDoc::Break(obj, doc1) => {
          let (_count, obj1) = _visit_obj(mem, obj, false);
          let doc2 = _visit_doc(mem, doc1);
          _break(mem, obj1, doc2)
        }
        DenullDoc::Line(obj) => {
          let (_count, obj1) = _visit_obj(mem, obj, false);
          _line(mem, obj1)
        }
      }
    }
    fn _visit_obj<'b, 'a: 'b>(
      mem: &'b Bump,
      obj: &'a DenullObj<'a>,
      under_seq: bool
    ) -> (Count, &'b DenullObj<'b>) {
      match obj {
        DenullObj::Term(term) |
        DenullObj::Fix(DenullFix::Term(term)) =>
          (Count::Zero, _term(mem, term)),
        DenullObj::Fix(fix) =>
          (Count::Zero, _fix(mem, fix)),
        DenullObj::Grp(obj1) => {
          let (_count, obj2) = _visit_obj(mem, obj1, false);
          (Count::Zero, _grp(mem, obj2))
        }
        DenullObj::Seq(obj1) =>
          if under_seq {
            _visit_obj(mem, obj1, true)
          } else {
            let (count, obj2) = _visit_obj(mem, obj1, true);
            match count {
              Count::Zero | Count::One => (count, obj2),
              Count::Many => (Count::Many, _seq(mem, obj2))
            }
          },
        DenullObj::Comp(left, right, pad) => {
          let (l_count, left1) = _visit_obj(mem, left, under_seq);
          let (r_count, right1) = _visit_obj(mem, right, under_seq);
          let count = _add(Count::One, _add(l_count, r_count));
          (count, _comp(mem, left1, right1, *pad))
        }
      }
    }
    _visit_doc(mem, doc)
  }
  fn _elim_grps<'b, 'a: 'b>(
    mem: &'b Bump,
    doc: &'a DenullDoc<'a>
  ) -> &'b DenullDoc<'b> {
    fn _visit_doc<'b, 'a: 'b>(
      mem: &'b Bump,
      doc: &'a DenullDoc<'a>
    ) -> &'b DenullDoc<'b> {
      match doc {
        DenullDoc::EOD =>
          _eod(mem),
        DenullDoc::Empty(doc1) => {
          let doc2 = _visit_doc(mem, doc1);
          _empty(mem, doc2)
        }
        DenullDoc::Break(obj, doc1) => {
          let (_count, obj1) = _visit_obj(mem, obj, true);
          let doc2 = _visit_doc(mem, doc1);
          _break(mem, obj1, doc2)
        }
        DenullDoc::Line(obj) => {
          let (_count, obj1) = _visit_obj(mem, obj, true);
          _line(mem, obj1)
        }
      }
    }
    fn _visit_obj<'b, 'a: 'b>(
      mem: &'b Bump,
      obj: &'a DenullObj<'a>,
      in_head: bool
    ) -> (Count, &'b DenullObj<'b>) {
      match obj {
        DenullObj::Term(term) |
        DenullObj::Fix(DenullFix::Term(term)) =>
          (Count::Zero, _term(mem, term)),
        DenullObj::Fix(fix) =>
          (Count::Zero, _fix(mem, fix)),
        DenullObj::Grp(obj1) =>
          if in_head  {
            _visit_obj(mem, obj1, true)
          } else {
            let (count, obj2) = _visit_obj(mem, obj1, false);
            match count {
              Count::Zero => (Count::Zero, obj2),
              Count::One | Count::Many => (Count::Zero, _grp(mem, obj2))
            }
          }
        DenullObj::Seq(obj1) => {
          let (count, obj2) = _visit_obj(mem, obj1, false);
          (count, _seq(mem, obj2))
        }
        DenullObj::Comp(left, right, pad) => {
          let (l_count, left1) = _visit_obj(mem, left, in_head);
          let (r_count, right1) = _visit_obj(mem, right, false);
          let count = _add(Count::One, _add(l_count, r_count));
          (count, _comp(mem, left1, right1, *pad))
        }
      }
    }
    _visit_doc(mem, doc)
  }
  let doc1 = _elim_seqs(mem, doc);
  _elim_grps(mem, doc1)
}

/*
  Reassociate after grp and seq removals
*/
fn _reassociate<'b, 'a: 'b>(
  mem: &'b Bump,
  doc: &'a DenullDoc<'a>
) -> &'b DenullDoc<'b> {
  fn _eod<'a>(
    mem: &'a Bump
  ) -> &'a DenullDoc<'a> {
    mem.alloc(DenullDoc::EOD)
  }
  fn _empty<'a>(
    mem: &'a Bump,
    doc: &'a DenullDoc<'a>
  ) -> &'a DenullDoc<'a> {
    mem.alloc(DenullDoc::Empty(doc))
  }
  fn _break<'a>(
    mem: &'a Bump,
    obj: &'a DenullObj<'a>,
    doc: &'a DenullDoc<'a>
  ) -> &'a DenullDoc<'a> {
    mem.alloc(DenullDoc::Break(obj, doc))
  }
  fn _line<'a>(
    mem: &'a Bump,
    obj: &'a DenullObj<'a>
  ) -> &'a DenullDoc<'a> {
    mem.alloc(DenullDoc::Line(obj))
  }
  fn _term<'a>(
    mem: &'a Bump,
    term: &'a DenullTerm<'a>
  ) -> &'a DenullObj<'a> {
    mem.alloc(DenullObj::Term(term))
  }
  fn _fix<'a>(
    mem: &'a Bump,
    fix: &'a DenullFix<'a>
  ) -> &'a DenullObj<'a> {
    mem.alloc(DenullObj::Fix(fix))
  }
  fn _grp<'a>(
    mem: &'a Bump,
    obj: &'a DenullObj<'a>
  ) -> &'a DenullObj<'a> {
    mem.alloc(DenullObj::Grp(obj))
  }
  fn _seq<'a>(
    mem: &'a Bump,
    obj: &'a DenullObj<'a>
  ) -> &'a DenullObj<'a> {
    mem.alloc(DenullObj::Seq(obj))
  }
  fn _comp<'a>(
    mem: &'a Bump,
    left: &'a DenullObj<'a>,
    right: &'a DenullObj<'a>,
    pad: bool
  ) -> &'a DenullObj<'a> {
    mem.alloc(DenullObj::Comp(left, right, pad))
  }
  fn __comp<'a>(
    mem: &'a Bump,
    pad: bool,
    right: &'a DenullObj<'a>,
    left: &'a DenullObj<'a>
  ) -> &'a DenullObj<'a> {
    _comp(mem, left, right, pad)
  }
  fn _visit_doc<'b, 'a: 'b>(
    mem: &'b Bump,
    doc: &'a DenullDoc<'a>
  ) -> &'b DenullDoc<'b> {
    match doc {
      DenullDoc::EOD =>
        _eod(mem),
      DenullDoc::Empty(doc1) => {
        let doc2 = _visit_doc(mem, doc1);
        _empty(mem, doc2)
      }
      DenullDoc::Break(obj, doc1) => {
        let partial = mem.alloc(move |_mem, obj1| obj1);
        _visit_obj(mem, obj, partial, mem.alloc(move |mem, obj2| {
        let doc2 = _visit_doc(mem, doc1);
        _break(mem, obj2, doc2)}))
      }
      DenullDoc::Line(obj) => {
        let partial = mem.alloc(|_mem, obj1| obj1);
        _visit_obj(mem, obj, partial, mem.alloc(|mem, obj2|
        _line(mem, obj2)))
      }
    }
  }
  fn _visit_obj<'b, 'a: 'b, R>(
    mem: &'b Bump,
    obj: &'a DenullObj<'a>,
    partial: &'b dyn Fn(&'b Bump, &'b DenullObj<'b>) -> &'b DenullObj<'b>,
    cont: &'b dyn Fn(&'b Bump, &'b DenullObj<'b>) -> R
  ) -> R {
    match obj {
      DenullObj::Term(term) =>
        cont(mem, partial(mem, _term(mem, term))),
      DenullObj::Fix(fix) =>
        cont(mem, partial(mem, _fix(mem, fix))),
      DenullObj::Grp(obj1) =>
        _visit_obj(mem, obj1, mem.alloc(|_mem, obj2| obj2),
        compose(mem, cont, compose(mem, partial, mem.alloc(|mem, obj3|
        _grp(mem, obj3))))),
      DenullObj::Seq(obj1) =>
        _visit_obj(mem, obj1, mem.alloc(|_mem, obj2| obj2),
        compose(mem, cont, compose(mem, partial, mem.alloc(|mem, obj3|
        _seq(mem, obj3))))),
      DenullObj::Comp(left, right, pad) =>
        _visit_obj(mem, right, partial, mem.alloc(move |mem, result|
        _visit_obj(mem, left,
          mem.alloc(move |mem, obj1| __comp(mem, *pad, result, obj1)),
          cont
        )))
    }
  }
  _visit_doc(mem, doc)
}

#[derive(Debug)]
enum FinalDoc<'a> {
  EOD,
  Empty(&'a FinalDoc<'a>),
  Break(&'a FinalDocObj<'a>, &'a FinalDoc<'a>),
  Line(&'a FinalDocObj<'a>)
}

#[derive(Debug)]
enum FinalDocObj<'a> {
  Text(&'a str),
  Fix(&'a FinalDocObjFix<'a>),
  Grp(&'a FinalDocObj<'a>),
  Seq(&'a FinalDocObj<'a>),
  Nest(&'a FinalDocObj<'a>),
  Pack(u64, &'a FinalDocObj<'a>),
  Comp(&'a FinalDocObj<'a>, &'a FinalDocObj<'a>, bool)
}

#[derive(Debug)]
enum FinalDocObjFix<'a> {
  Text(&'a str),
  Comp(&'a FinalDocObjFix<'a>, &'a FinalDocObjFix<'a>, bool)
}

#[derive(Debug, Copy, Clone)]
enum Prop {
  Nest,
  Pack(u64)
}

/*
  Rescope nest and pack.
*/
fn _rescope<'b, 'a: 'b>(
  mem: &'b Bump,
  doc: &'a DenullDoc<'a>
) -> &'b FinalDoc<'b> {
  fn _eod<'a>(
    mem: &'a Bump
  ) -> &'a FinalDoc<'a> {
    mem.alloc(FinalDoc::EOD)
  }
  fn _empty<'a>(
    mem: &'a Bump,
    doc: &'a FinalDoc<'a>
  ) -> &'a FinalDoc<'a> {
    mem.alloc(FinalDoc::Empty(doc))
  }
  fn _break<'a>(
    mem: &'a Bump,
    obj: &'a FinalDocObj<'a>,
    doc: &'a FinalDoc<'a>
  ) -> &'a FinalDoc<'a> {
    mem.alloc(FinalDoc::Break(obj, doc))
  }
  fn _line<'a>(
    mem: &'a Bump,
    obj: &'a FinalDocObj<'a>
  ) -> &'a FinalDoc<'a> {
    mem.alloc(FinalDoc::Line(obj))
  }
  fn _text<'a>(
    mem: &'a Bump,
    data: &'a str
  ) -> &'a FinalDocObj<'a> {
    mem.alloc(FinalDocObj::Text(data))
  }
  fn _fix<'a>(
    mem: &'a Bump,
    fix: &'a FinalDocObjFix<'a>
  ) -> &'a FinalDocObj<'a> {
    mem.alloc(FinalDocObj::Fix(fix))
  }
  fn _grp<'a>(
    mem: &'a Bump,
    obj: &'a FinalDocObj<'a>
  ) -> &'a FinalDocObj<'a> {
    mem.alloc(FinalDocObj::Grp(obj))
  }
  fn _seq<'a>(
    mem: &'a Bump,
    obj: &'a FinalDocObj<'a>
  ) -> &'a FinalDocObj<'a> {
    mem.alloc(FinalDocObj::Seq(obj))
  }
  fn _nest<'a>(
    mem: &'a Bump,
    obj: &'a FinalDocObj<'a>
  ) -> &'a FinalDocObj<'a> {
    mem.alloc(FinalDocObj::Nest(obj))
  }
  fn _pack<'a>(
    mem: &'a Bump,
    index: u64,
    obj: &'a FinalDocObj<'a>
  ) -> &'a FinalDocObj<'a> {
    mem.alloc(FinalDocObj::Pack(index, obj))
  }
  fn _comp<'a>(
    mem: &'a Bump,
    left: &'a FinalDocObj<'a>,
    right: &'a FinalDocObj<'a>,
    pad: bool
  ) -> &'a FinalDocObj<'a> {
    mem.alloc(FinalDocObj::Comp(left, right, pad))
  }
  fn _fix_text<'a>(
    mem: &'a Bump,
    data: &'a str
  ) -> &'a FinalDocObjFix<'a> {
    mem.alloc(FinalDocObjFix::Text(data))
  }
  fn _fix_comp<'a>(
    mem: &'a Bump,
    left: &'a FinalDocObjFix<'a>,
    right: &'a FinalDocObjFix<'a>,
    pad: bool
  ) -> &'a FinalDocObjFix<'a> {
    mem.alloc(FinalDocObjFix::Comp(left, right, pad))
  }
  fn _prop_pack(index: u64) -> Prop {
    Prop::Pack(index)
  }
  fn _join_props<'b, 'a: 'b>(
    mem: &'b Bump,
    l: &'a List<'a, Prop>,
    r: &'a List<'a, Prop>
  ) -> (
    &'b List<'b, Prop>,
    &'b List<'b, Prop>,
    &'b List<'b, Prop>
  ) {
    fn _visit<'b, 'a: 'b>(
      mem: &'b Bump,
      l: &'a List<'a, Prop>,
      r: &'a List<'a, Prop>,
      c: &'a dyn Fn(&'b Bump, &'a List<'a, Prop>) -> &'a List<'a, Prop>
    ) -> (
      &'b List<'b, Prop>,
      &'b List<'b, Prop>,
      &'b List<'b, Prop>
    ) {
      match (l, r) {
        ( List::Cons(_, Prop::Nest, l1)
        , List::Cons(_, Prop::Nest, r1)) => {
          let c1 = compose(mem, c, mem.alloc(|mem, props|
            _list::cons(mem, Prop::Nest, props)));
          _visit(mem, l1, r1, c1)
        }
        ( List::Cons(_, Prop::Pack(l_index), l1)
        , List::Cons(_, Prop::Pack(r_index), r1)) =>
          if l_index != r_index {
            (l, r, c(mem, _list::nil(mem)))
          } else {
            let c1 = compose(mem, c, mem.alloc(|mem, props|
              _list::cons(mem, _prop_pack(*l_index), props)));
            _visit(mem, l1, r1, c1)
          }
        (_, _) =>
          (l, r, c(mem, _list::nil(mem)))
      }
    }
    _visit(mem, l, r, mem.alloc(|_mem, props| props))
  }
  fn _apply_props<'b, 'a: 'b, R>(
    mem: &'b Bump,
    props: &'a List<'a, Prop>,
    term: &'a FinalDocObj<'a>,
    cont: &'b dyn Fn(&'b Bump, &'b FinalDocObj<'b>) -> R
  ) -> R {
    match props {
      List::Nil => cont(mem, term),
      List::Cons(_, Prop::Nest, props1) =>
        _apply_props(mem, props1, term, compose(mem, cont, mem.alloc(|mem, obj|
        _nest(mem, obj)))),
      List::Cons(_, Prop::Pack(index), props1) =>
        _apply_props(mem, props1, term, compose(mem, cont, mem.alloc(|mem, obj|
        _pack(mem, *index, obj))))
    }
  }
  fn _visit_doc<'b, 'a: 'b>(
    mem: &'b Bump,
    doc: &'a DenullDoc<'a>
  ) -> &'b FinalDoc<'b> {
    match doc {
      DenullDoc::EOD =>
        _eod(mem),
      DenullDoc::Empty(doc1) => {
        let doc2 = _visit_doc(mem, doc1);
        _empty(mem, doc2)
      }
      DenullDoc::Break(obj, doc1) => {
        let (props, obj1) = _visit_obj(mem, obj);
        _apply_props(mem, props, obj1, mem.alloc(move |mem, obj2| {
        let doc2 = _visit_doc(mem, doc1);
        _break(mem, obj2, doc2)}))
      }
      DenullDoc::Line(obj) => {
        let (props, obj1) = _visit_obj(mem, obj);
        _apply_props(mem, props, obj1, mem.alloc(|mem, obj2|
        _line(mem, obj2)))
      }
    }
  }
  fn _visit_obj<'b, 'a: 'b>(
    mem: &'b Bump,
    obj: &'a DenullObj<'a>
  ) -> (
    &'b List<'b, Prop>,
    &'b FinalDocObj<'b>
  ) {
    match obj {
      DenullObj::Term(term) =>
        _visit_term(mem, term, mem.alloc(|_mem, props| props)),
      DenullObj::Fix(fix) => {
        let (props, fix1) = _visit_fix(mem, fix);
        (props, _fix(mem, fix1))
      }
      DenullObj::Grp(obj1) => {
        let (props, obj2) = _visit_obj(mem, obj1);
        (props, _grp(mem, obj2))
      }
      DenullObj::Seq(obj1) => {
        let (props, obj2) = _visit_obj(mem, obj1);
        (props, _seq(mem, obj2))
      }
      DenullObj::Comp(left, right, pad) => {
        let (l_props, left1) = _visit_obj(mem, left);
        let (r_props, right1) = _visit_obj(mem, right);
        let (l_props1, r_props1, c_props) = _join_props(mem, l_props, r_props);
        _apply_props(mem, l_props1, left1, mem.alloc(move |mem, left2|
        _apply_props(mem, r_props1, right1, mem.alloc(move |mem, right2|
        (c_props, _comp(mem, left2, right2, *pad))))))
      }
    }
  }
  fn _visit_fix<'b, 'a: 'b>(
    mem: &'b Bump,
    fix: &'a DenullFix<'a>
  ) -> (
    &'b List<'b, Prop>,
    &'b FinalDocObjFix<'b>
  ) {
    match fix {
      DenullFix::Term(term) =>
        _visit_fix_term(mem, term, mem.alloc(|_mem, props| props)),
      DenullFix::Comp(left, right, pad) => {
        let (l_props, left1) = _visit_fix(mem, left);
        let (_r_props, right1) = _visit_fix(mem, right);
        (l_props, _fix_comp(mem, left1, right1, *pad))
      }
    }
  }
  fn _visit_term<'b, 'a: 'b>(
    mem: &'b Bump,
    term: &'a DenullTerm<'a>,
    result: &'b dyn Fn(&'b Bump, &'b List<'b, Prop>) -> &'b List<'b, Prop>
  ) -> (
    &'b List<'b, Prop>,
    &'b FinalDocObj<'b>
  ) {
    match term {
      DenullTerm::Text(data) =>
        (result(mem, _list::nil(mem)), _text(mem, data)),
      DenullTerm::Nest(term1) => {
        let result1 = compose(mem, result, mem.alloc(|mem, props|
          _list::cons(mem, Prop::Nest, props)));
        _visit_term(mem, term1, result1)
      }
      DenullTerm::Pack (index, term1) => {
        let result1 = compose(mem, result, mem.alloc(|mem, props|
          _list::cons(mem, _prop_pack(*index), props)));
        _visit_term(mem, term1, result1)
      }
    }
  }
  fn _visit_fix_term<'b, 'a: 'b>(
    mem: &'b Bump,
    term: &'a DenullTerm<'a>,
    result: &'b dyn Fn(&'b Bump, &'b List<'b, Prop>) -> &'b List<'b, Prop>
  ) -> (
    &'b List<'b, Prop>,
    &'b FinalDocObjFix<'b>
  ) {
    match term {
      DenullTerm::Text(data) =>
        (result(mem, _list::nil(mem)), _fix_text(mem, data)),
      DenullTerm::Nest(term1) => {
        let result1 = compose(mem, result, mem.alloc(|mem, props|
          _list::cons(mem, Prop::Nest, props)));
        _visit_fix_term(mem, term1, result1)
      }
      DenullTerm::Pack(index, term1) => {
        let result1 = compose(mem, result, mem.alloc(|mem, props|
          _list::cons(mem, _prop_pack(*index), props)));
        _visit_fix_term(mem, term1, result1)
      }
    }
  }
  _visit_doc(mem, doc)
}

#[derive(Debug, Clone)]
pub enum Doc {
  EOD,
  Empty(Box<Doc>),
  Break(Box<DocObj>, Box<Doc>),
  Line(Box<DocObj>)
}

#[derive(Debug, Clone)]
pub enum DocObj {
  Text(String),
  Fix(Box<DocObjFix>),
  Grp(Box<DocObj>),
  Seq(Box<DocObj>),
  Nest(Box<DocObj>),
  Pack(u64, Box<DocObj>),
  Comp(Box<DocObj>, Box<DocObj>, bool)
}

#[derive(Debug, Clone)]
pub enum DocObjFix {
  Text(String),
  Comp(Box<DocObjFix>, Box<DocObjFix>, bool)
}

impl fmt::Display for Doc {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    fn _print_doc(
      doc: Box<Doc>
    ) -> String {
      match doc {
        box Doc::EOD => "EOD".to_string(),
        box Doc::Empty(doc1) => {
          let doc_s = _print_doc(doc1);
          format!("Empty\n{}", doc_s)
        }
        box Doc::Break(obj, doc1) => {
          let obj_s = _print_obj(obj);
          let doc1_s = _print_doc(doc1);
          format!("Break {}\n{}", obj_s, doc1_s)
        }
        box Doc::Line(obj) => {
          let obj_s = _print_obj(obj);
          format!("Line {}", obj_s)
        }
      }
    }
    fn _print_obj(
      obj: Box<DocObj>
    ) -> String {
      match obj {
        box DocObj::Text(data) =>
          format!("(Text \"{}\")", data),
        box DocObj::Fix(obj1) => {
          let obj_s = _print_fix(obj1);
          format!("(Fix {})", obj_s)
        }
        box DocObj::Grp(obj1) => {
          let obj_s = _print_obj(obj1);
          format!("(Grp {})", obj_s)
        }
        box DocObj::Seq(obj1) => {
          let obj_s = _print_obj(obj1);
          format!("(Seq {})", obj_s)
        }
        box DocObj::Nest(obj1) => {
          let obj_s = _print_obj(obj1);
          format!("(Nest {})", obj_s)
        }
        box DocObj::Pack(index, obj1) => {
          let obj_s = _print_obj(obj1);
          format!("(Pack {} {})", index, obj_s)
        }
        box DocObj::Comp(left, right, pad) => {
          let left_s = _print_obj(left);
          let right_s = _print_obj(right);
          format!("(Comp {} {} {})", left_s, right_s, pad)
        }
      }
    }
    fn _print_fix(
      obj: Box<DocObjFix>
    ) -> String {
      match obj {
        box DocObjFix::Text(data) =>
          format!("(Text \"{}\")", data),
        box DocObjFix::Comp(left, right, pad) => {
          let left_s = _print_fix(left);
          let right_s = _print_fix(right);
          format!("(Comp {} {} {})", left_s, right_s, pad)
        }
      }
    }
    write!(f, "{}", _print_doc(Box::new(self.clone())))
  }
}

fn _move_to_heap<'a>(
  doc: &'a FinalDoc<'a>
) -> Box<Doc> {
  fn _visit_doc<'a>(
    doc: &'a FinalDoc<'a>
  ) -> Box<Doc> {
    match doc {
      FinalDoc::EOD => Box::new(Doc::EOD),
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
  fn _visit_obj<'a>(
    obj: &'a FinalDocObj<'a>
  ) -> Box<DocObj> {
    match obj {
      FinalDocObj::Text(data) =>
        Box::new(DocObj::Text(data.to_string())),
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
  fn _visit_fix<'a>(
    fix: &'a FinalDocObjFix<'a>
  ) -> Box<DocObjFix> {
    match fix {
      FinalDocObjFix::Text(data) =>
        Box::new(DocObjFix::Text(data.to_string())),
      FinalDocObjFix::Comp(left, right, pad) => {
        let left1 = _visit_fix(left);
        let right1 = _visit_fix(right);
        Box::new(DocObjFix::Comp(left1, right1, *pad))
      }
    }
  }
  _visit_doc(doc)
}

/// A function for compiling layouts into documents optimized for rendering, takes a `Box<Layout>` and gives a `Box<Doc>`.
///
/// # Examples
/// ```
/// use typeset::{text, comp, compile};
///
/// let layout = comp(
///   text("foo".to_string()),
///   text("bar".to_string()),
///   false, false
/// );
/// let document = compile(layout);
/// ```
pub fn compile(
  layout: Box<Layout>
) -> Box<Doc> {
  let mem = Bump::new();
  let layout1 = _broken(&mem, layout);
  let layout2 = _serialize(&mem, layout1);
  let doc = _linearize(&mem, layout2);
  let doc1 = _fixed(&mem, doc);
  let doc2 = _structurize(&mem, doc1);
  let doc3 = _denull(&mem, doc2);
  let doc4 = _identities(&mem, doc3);
  let doc5 = _reassociate(&mem, doc4);
  let doc6 = _rescope(&mem, doc5);
  _move_to_heap(doc6)
}

#[derive(Debug, Copy, Clone)]
struct State<'a> {
  width: usize,
  tab: usize,
  head: bool,
  broken: bool,
  lvl: usize,
  pos: usize,
  marks: &'a Map<'a, usize, usize>
}

fn _make_state<'a>(
  mem: &'a Bump,
  width: usize,
  tab: usize
) -> State<'a> {
  State {
    width: width,
    tab: tab,
    head: true,
    broken: false,
    lvl: 0,
    pos: 0,
    marks: _map::empty(mem)
  }
}

fn _inc_pos<'a>(
  n: usize,
  state: State<'a>
) -> State<'a> {
  State {
    pos: state.pos + n,
    ..state
  }
}

fn _indent<'a>(
  tab: usize,
  state: State<'a>
) -> State<'a> {
  if tab <= 0 { state } else {
  let lvl = state.lvl;
  let lvl1 = lvl + (tab - (lvl % tab));
  State { lvl: lvl1, ..state }}
}

fn _newline<'a>(
  state: State<'a>
) -> State<'a> {
  State {
    head: true,
    pos: 0,
    ..state
  }
}

fn _reset<'a>(
  state: State<'a>
) -> State<'a> {
  State {
    head: true,
    broken: false,
    pos: 0,
    ..state
  }
}

fn _get_offset<'a>(
  state: State<'a>
) -> usize {
  if !state.head { 0 } else {
  max(0, state.lvl - state.pos)}
}

/// A function for rendering documents, takes a `Box<Doc>`, a tab indentation size and a output buffer target width, and gives a `String`.
///
/// # Examples
/// ```
/// use typeset::{text, comp, compile, render};
///
/// let layout = comp(
///   text("foo".to_string()),
///   text("bar".to_string()),
///   false, false
/// );
/// let document = compile(layout);
/// println!("{}", render(document, 2, 80));
/// ```
pub fn render(
  doc: Box<Doc>,
  tab: usize,
  width: usize
) -> String {
  fn _whitespace(n: usize) -> String { " ".repeat(n) }
  fn _pad<'a>(
    n: usize,
    result: String
  ) -> String {
    result + &_whitespace(n)
  }
  fn _measure<'b, 'a: 'b>(
    mem: &'b Bump,
    obj: &Box<DocObj>,
    state: State<'a>
  ) -> usize {
    fn _visit_obj<'b, 'a: 'b>(
      mem: &'b Bump,
      obj: &Box<DocObj>,
      state: State<'a>
    ) -> State<'b> {
      match obj {
        box DocObj::Text(data) => _inc_pos(data.len(), state),
        box DocObj::Fix(fix) => _visit_fix(fix, state),
        box DocObj::Grp(obj1) => _visit_obj(mem, obj1, state),
        box DocObj::Seq(obj1) => _visit_obj(mem, obj1, state),
        box DocObj::Nest(obj1) => {
          let lvl = state.lvl;
          let state1 = _indent(state.tab, state);
          let offset = _get_offset(state1);
          let state2 = _inc_pos(offset, state1);
          let state3 = _visit_obj(mem, obj1, state2);
          State { lvl: lvl, ..state3 }
        }
        box DocObj::Pack(index, obj1) => {
          let index = *index as usize;
          let lvl = state.lvl;
          let marks = state.marks;
          match marks.lookup(&total, index) {
            None => {
              let pos = state.pos;
              let marks1 = marks.insert(mem, &total, index, pos);
              let state1 = State { marks: marks1, ..state };
              let state2 = State { lvl: max(lvl, pos), ..state1 };
              let state3 = _visit_obj(mem, obj1, state2);
              State { lvl: lvl, ..state3 }
            }
            Some(lvl1) => {
              let state1 = State { lvl: max(lvl, lvl1), ..state };
              let offset = _get_offset(state1);
              let state2 = _inc_pos(offset, state1);
              let state3 = _visit_obj(mem, obj1, state2);
              State { lvl: lvl, ..state3 }
            }
          }
        }
        box DocObj::Comp(left, right, pad) => {
          let state1 = _visit_obj(mem, left, state);
          let state2 = _inc_pos(if *pad { 1 } else { 0 }, state1);
          let head = state2.head;
          let state3 = State { head: false, ..state2 };
          let state4 = _visit_obj(mem, right, state3);
          State { head: head, ..state4 }
        }
      }
    }
    fn _visit_fix<'b, 'a: 'b>(
      fix: &Box<DocObjFix>,
      state: State<'a>
    ) -> State<'a> {
      match fix {
        box DocObjFix::Text(data) =>
          _inc_pos(data.len(), state),
        box DocObjFix::Comp(left, right, pad) => {
          let state1 = _visit_fix(left, state);
          let state2 = _inc_pos(if *pad { 1 } else { 0 }, state1);
          _visit_fix(right, state2)
        }
      }
    }
    let state1 = _visit_obj(mem, obj, state);
    state1.pos
  }
  fn _next_comp<'b, 'a: 'b>(
    mem: &'b Bump,
    obj: &Box<DocObj>,
    state: State<'a>
  ) -> usize {
    fn _visit_obj<'b, 'a: 'b>(
      mem: &'b Bump,
      obj: &Box<DocObj>,
      state: State<'a>
    ) -> State<'b> {
      match obj {
        box DocObj::Text(data) =>
          _inc_pos(data.len(), state),
        box DocObj::Fix(fix) =>
          _visit_fix(mem, fix, state),
        box DocObj::Grp(obj1) => {
          let head = state.head;
          if head { _visit_obj(mem, obj1, state) } else {
          let obj_end_pos = _measure(mem, &obj1, state);
          State { pos: obj_end_pos, ..state }}
        }
        box DocObj::Seq(obj1) =>
          _visit_obj(mem, obj1, state),
        box DocObj::Nest(obj1) => {
          let lvl = state.lvl;
          let state1 = _indent(state.tab, state);
          let offset = _get_offset(state1);
          let state2 = _inc_pos(offset, state1);
          let state3 = _visit_obj(mem, obj1, state2);
          State { lvl: lvl, ..state3 }
        }
        box DocObj::Pack(index, obj1) => {
          let index = *index as usize;
          let lvl = state.lvl;
          let marks = state.marks;
          match marks.lookup(&total, index) {
            None => {
              let pos = state.pos;
              let marks1 = marks.insert(mem, &total, index, pos);
              let state1 = State { marks: marks1, ..state };
              let state2 = State { lvl: max(lvl, pos), ..state1 };
              let state3 = _visit_obj(mem, obj1, state2);
              State { lvl: lvl, ..state3 }
            }
            Some(lvl1) => {
              let state1 = State { lvl: max(lvl, lvl1), ..state };
              let offset = _get_offset(state1);
              let state2 = _inc_pos(offset, state1);
              let state3 = _visit_obj(mem, obj1, state2);
              State { lvl: lvl, ..state3 }
            }
          }
        }
        box DocObj::Comp(left, _right, _pad) =>
          _visit_obj(mem, left, state)
      }
    }
    fn _visit_fix<'b, 'a: 'b>(
      mem: &'b Bump,
      fix: &Box<DocObjFix>,
      state: State<'a>
    ) -> State<'a> {
      match fix {
        box DocObjFix::Text(data) =>
          _inc_pos(data.len(), state),
        box DocObjFix::Comp(left, right, pad) => {
          let state1 = _visit_fix(mem, left, state);
          let state2 = _inc_pos(if *pad { 1 } else { 0 }, state1);
          _visit_fix(mem, right, state2)
        }
      }
    }
    let state1 = _visit_obj(mem, obj, state);
    state1.pos
  }
  fn _will_fit<'b, 'a: 'b>(
    mem: &'b Bump,
    obj: &Box<DocObj>,
    state: State
  ) -> bool {
    let obj_end_pos = _measure(mem, obj, state);
    obj_end_pos <= state.width
  }
  fn _should_break<'b, 'a: 'b>(
    mem: &'b Bump,
    obj: &Box<DocObj>,
    state: State
  ) -> bool {
    let broken = state.broken;
    if broken { true } else {
    let next_comp_pos = _next_comp(mem, obj, state);
    state.width < next_comp_pos }
  }
  fn _visit_doc<'b, 'a: 'b>(
    mem: &'b Bump,
    doc: Box<Doc>,
    state: State<'a>
  ) -> (State<'b>, String) {
    let state1 = _reset(state);
    match doc {
      box Doc::EOD =>
        (state1, "".to_string()),
      box Doc::Empty(doc1) => {
        let (state2, doc2) = _visit_doc(mem, doc1, state1);
        (state2, format!("\n{}", doc2))
      }
      box Doc::Break(obj, doc1) => {
        let (state2, obj1) = _visit_obj(mem, obj, state1, "".to_string());
        let state3 = _reset(state2);
        let (state4, doc2) = _visit_doc(mem, doc1, state3);
        (state4, format!("{}\n{}", obj1, doc2))
      }
      box Doc::Line(obj) =>
        _visit_obj(mem, obj, state1, "".to_string())
    }
  }
  fn _visit_obj<'b, 'a: 'b>(
    mem: &'b Bump,
    obj: Box<DocObj>,
    state: State<'a>,
    result: String
  ) -> (State<'b>, String) {
    match obj {
      box DocObj::Text(data) => {
        let state1 = _inc_pos(data.len(), state);
        (state1, result.clone() + &data)
      }
      box DocObj::Fix(fix) =>
        _visit_fix(mem, fix, state, result),
      box DocObj::Grp(obj1) => {
        let broken = state.broken;
        let state1 = State { broken: false, ..state };
        let (state2, result1) = _visit_obj(mem, obj1, state1, result.clone());
        let state3 = State { broken: broken, ..state2 };
        (state3, result1.clone())
      }
      box DocObj::Seq(obj1) =>
        if _will_fit(mem, &obj1, state) {
          _visit_obj(mem, obj1, state, result)
        } else {
          let broken = state.broken;
          let state1 = State { broken: true, ..state };
          let (state2, result1) = _visit_obj(mem, obj1, state1, result.clone());
          let state3 = State { broken: broken, ..state2 };
          (state3, result1.clone())
        }
      box DocObj::Nest(obj1) => {
        let lvl = state.lvl;
        let state1 = _indent(state.tab, state);
        let offset = _get_offset(state1);
        let state2 = _inc_pos(offset, state1);
        let result1 = _pad(offset, result.clone());
        let (state3, result2) = _visit_obj(mem, obj1, state2, result1.clone());
        let state4 = State { lvl: lvl, ..state3 };
        (state4, result2.clone())
      }
      box DocObj::Pack(index, obj1) => {
        let index = index as usize;
        let lvl = state.lvl;
        let marks = state.marks;
        match marks.lookup(&total, index) {
          None => {
            let pos = state.pos;
            let marks1 = marks.insert(mem, &total, index, pos);
            let state1 = State { marks: marks1, ..state };
            let state2 = State { lvl: max(lvl, pos), ..state1 };
            let (state3, result1) = _visit_obj(
              mem, obj1, state2, result.clone()
            );
            let state4 = State { lvl: lvl, ..state3 };
            (state4, result1.clone())
          }
          Some(lvl1) => {
            let state1 = State { lvl: max(lvl, lvl1), ..state };
            let offset = _get_offset(state1);
            let state2 = _inc_pos(offset, state1);
            let result1 = _pad(offset, result.clone());
            let (state3, result2) = _visit_obj(
              mem, obj1, state2, result1.clone()
            );
            let state4 = State { lvl: lvl, ..state3 };
            (state4, result2.clone())
          }
        }
      }
      box DocObj::Comp(left, right, pad) => {
        let (state1, result1) = _visit_obj(mem, left, state, result);
        let state2 = _inc_pos(if pad { 1 } else { 0 }, state1);
        let state3 = State { head: false, ..state2 };
        if _should_break(mem, &right, state3) {
          let state2 = _newline(state1);
          let offset = _get_offset(state2);
          let state3 = _inc_pos(offset, state2);
          let result2 = _pad(offset, result1.clone() + "\n");
          _visit_obj(mem, right, state3, result2)
        } else {
          let result2 = _pad(if pad { 1 } else { 0 }, result1.clone());
          _visit_obj(mem, right, state3, result2)
        }
      }
    }
  }
  fn _visit_fix<'b, 'a: 'b>(
    mem: &'b Bump,
    fix: Box<DocObjFix>,
    state: State<'a>,
    result: String
  ) -> (State<'a>, String) {
    match fix {
      box DocObjFix::Text(data) => {
        let state1 = _inc_pos(data.len(), state);
        (state1, result.clone() + &data)
      }
      box DocObjFix::Comp(left, right, pad) => {
        let (state1, result1) = _visit_fix(mem, left, state, result);
        let padding = if pad { 1 } else { 0 };
        let result2 = _pad(padding, result1);
        let state2 = _inc_pos(padding, state1);
        _visit_fix(mem, right, state2, result2.clone())
      }
    }
  }
  let mem = Bump::new();
  let (_state, result) = _visit_doc(&mem, doc, _make_state(&mem, width, tab));
  result
}