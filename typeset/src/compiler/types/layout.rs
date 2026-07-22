use super::traversal::DismantleTree;
use std::fmt;
use std::mem;

/// Attribute structure for Layout compositions
#[derive(Debug, Copy, Clone)]
pub struct Attr {
    pub pad: bool,
    pub fix: bool,
}

/// Whether a composition puts a space between its two operands when they share
/// a line — the padding axis of [`comp`](crate::comp).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Pad {
    /// No space between the operands (`foo` then `(` renders as `foo(`).
    Unpadded,
    /// A single space between the operands (`foo` then `bar` renders as
    /// `foo bar`).
    Padded,
}

/// Whether a composition may break across lines — the break axis of
/// [`comp`](crate::comp). `Fixed` is the composition-level analogue of wrapping
/// in [`fix`](crate::fix).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Break {
    /// The composition may move its right operand to the next line when it does
    /// not fit within the target width.
    Breakable,
    /// The composition never breaks; both operands always stay on one line.
    Fixed,
}

impl Pad {
    /// The internal boolean encoding (`Padded` is `true`).
    pub(crate) fn is_padded(self) -> bool {
        matches!(self, Pad::Padded)
    }
}

impl Break {
    /// The internal boolean encoding (`Fixed` is `true`, matching `Attr::fix`).
    pub(crate) fn is_fixed(self) -> bool {
        matches!(self, Break::Fixed)
    }
}

/// Layout AST - the input language for the compiler
///
/// `Clone`, `Drop`, `Debug`, and `Display` are implemented iteratively below
/// rather than derived: a derived (recursive) impl would overflow the native
/// stack on a deeply nested layout, the same hazard the compiler passes and
/// renderer avoid.
#[derive(Default)]
pub enum Layout {
    #[default]
    Null,
    Text(String),
    Fix(Box<Layout>),
    Grp(Box<Layout>),
    Seq(Box<Layout>),
    Nest(Box<Layout>),
    Pack(Box<Layout>),
    Line(Box<Layout>, Box<Layout>),
    Comp(Box<Layout>, Box<Layout>, Attr),
}

/// Move a node's children onto the worklist (taking them out of their `Box` and
/// leaving a `Null` placeholder), so the recursive drop of each box terminates
/// in O(1) and the tree is freed with a heap-allocated stack instead of the
/// native one. See [`DismantleTree`] for the shared driver.
impl DismantleTree for Layout {
    fn dismantle(&mut self, stack: &mut Vec<Self>) {
        match self {
            Layout::Null | Layout::Text(_) => {}
            Layout::Fix(l)
            | Layout::Grp(l)
            | Layout::Seq(l)
            | Layout::Nest(l)
            | Layout::Pack(l) => {
                stack.push(*mem::take(l));
            }
            Layout::Line(left, right) | Layout::Comp(left, right, _) => {
                stack.push(*mem::take(left));
                stack.push(*mem::take(right));
            }
        }
    }
}

impl Drop for Layout {
    fn drop(&mut self) {
        self.drain();
    }
}

/// Deep-copy a layout iteratively (bottom-up build with task/result stacks).
fn clone_layout(layout: &Layout) -> Box<Layout> {
    enum Task<'a> {
        Visit(&'a Layout),
        Fix,
        Grp,
        Seq,
        Nest,
        Pack,
        Line,
        Comp(Attr),
    }
    let mut tasks: Vec<Task> = vec![Task::Visit(layout)];
    let mut out: Vec<Box<Layout>> = Vec::new();
    while let Some(task) = tasks.pop() {
        match task {
            Task::Visit(l) => match l {
                Layout::Null => out.push(Box::new(Layout::Null)),
                Layout::Text(data) => out.push(Box::new(Layout::Text(data.clone()))),
                Layout::Fix(l1) => {
                    tasks.push(Task::Fix);
                    tasks.push(Task::Visit(l1));
                }
                Layout::Grp(l1) => {
                    tasks.push(Task::Grp);
                    tasks.push(Task::Visit(l1));
                }
                Layout::Seq(l1) => {
                    tasks.push(Task::Seq);
                    tasks.push(Task::Visit(l1));
                }
                Layout::Nest(l1) => {
                    tasks.push(Task::Nest);
                    tasks.push(Task::Visit(l1));
                }
                Layout::Pack(l1) => {
                    tasks.push(Task::Pack);
                    tasks.push(Task::Visit(l1));
                }
                Layout::Line(left, right) => {
                    tasks.push(Task::Line);
                    tasks.push(Task::Visit(right));
                    tasks.push(Task::Visit(left));
                }
                Layout::Comp(left, right, attr) => {
                    tasks.push(Task::Comp(*attr));
                    tasks.push(Task::Visit(right));
                    tasks.push(Task::Visit(left));
                }
            },
            Task::Fix => {
                let inner = out.pop().expect("fix operand");
                out.push(Box::new(Layout::Fix(inner)));
            }
            Task::Grp => {
                let inner = out.pop().expect("grp operand");
                out.push(Box::new(Layout::Grp(inner)));
            }
            Task::Seq => {
                let inner = out.pop().expect("seq operand");
                out.push(Box::new(Layout::Seq(inner)));
            }
            Task::Nest => {
                let inner = out.pop().expect("nest operand");
                out.push(Box::new(Layout::Nest(inner)));
            }
            Task::Pack => {
                let inner = out.pop().expect("pack operand");
                out.push(Box::new(Layout::Pack(inner)));
            }
            Task::Line => {
                let right = out.pop().expect("line: right operand");
                let left = out.pop().expect("line: left operand");
                out.push(Box::new(Layout::Line(left, right)));
            }
            Task::Comp(attr) => {
                let right = out.pop().expect("comp: right operand");
                let left = out.pop().expect("comp: left operand");
                out.push(Box::new(Layout::Comp(left, right, attr)));
            }
        }
    }
    out.pop().expect("clone produced no result")
}

impl Clone for Layout {
    fn clone(&self) -> Self {
        *clone_layout(self)
    }
}

impl fmt::Debug for Layout {
    // Iterative like `Clone` and `Drop`: a derived (recursive) `Debug` would
    // overflow the native stack on a deep layout. Prints the derived-style
    // compact form; the alternate (`{:#?}`) indented form is not reproduced
    // and falls back to this compact form.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        enum Task<'a> {
            Visit(&'a Layout),
            Lit(&'static str),
            Owned(String),
        }
        let mut stack: Vec<Task> = vec![Task::Visit(self)];
        while let Some(task) = stack.pop() {
            match task {
                Task::Lit(s) => f.write_str(s)?,
                Task::Owned(s) => f.write_str(&s)?,
                Task::Visit(l) => match l {
                    Layout::Null => f.write_str("Null")?,
                    Layout::Text(data) => f.write_str(&format!("Text({:?})", data))?,
                    Layout::Fix(l1) => {
                        stack.push(Task::Lit(")"));
                        stack.push(Task::Visit(l1));
                        stack.push(Task::Lit("Fix("));
                    }
                    Layout::Grp(l1) => {
                        stack.push(Task::Lit(")"));
                        stack.push(Task::Visit(l1));
                        stack.push(Task::Lit("Grp("));
                    }
                    Layout::Seq(l1) => {
                        stack.push(Task::Lit(")"));
                        stack.push(Task::Visit(l1));
                        stack.push(Task::Lit("Seq("));
                    }
                    Layout::Nest(l1) => {
                        stack.push(Task::Lit(")"));
                        stack.push(Task::Visit(l1));
                        stack.push(Task::Lit("Nest("));
                    }
                    Layout::Pack(l1) => {
                        stack.push(Task::Lit(")"));
                        stack.push(Task::Visit(l1));
                        stack.push(Task::Lit("Pack("));
                    }
                    Layout::Line(left, right) => {
                        stack.push(Task::Lit(")"));
                        stack.push(Task::Visit(right));
                        stack.push(Task::Lit(", "));
                        stack.push(Task::Visit(left));
                        stack.push(Task::Lit("Line("));
                    }
                    Layout::Comp(left, right, attr) => {
                        stack.push(Task::Owned(format!(", {:?})", attr)));
                        stack.push(Task::Visit(right));
                        stack.push(Task::Lit(", "));
                        stack.push(Task::Visit(left));
                        stack.push(Task::Lit("Comp("));
                    }
                },
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Past where a recursive clone/drop/print aborted.
    const DEEP: usize = 50_000;

    fn deep_nest(depth: usize) -> Box<Layout> {
        let mut layout = Box::new(Layout::Text("x".to_string()));
        for _ in 0..depth {
            layout = Box::new(Layout::Nest(layout));
        }
        layout
    }

    #[test]
    fn deep_drop_is_iterative() {
        let layout = deep_nest(DEEP);
        drop(layout);
    }

    #[test]
    fn deep_comp_drop_is_iterative() {
        let mut layout = Box::new(Layout::Text("x".to_string()));
        for _ in 0..DEEP {
            layout = Box::new(Layout::Comp(
                layout,
                Box::new(Layout::Text("y".to_string())),
                Attr {
                    pad: false,
                    fix: false,
                },
            ));
        }
        drop(layout);
    }

    #[test]
    fn deep_clone_is_iterative() {
        let layout = deep_nest(DEEP);
        let cloned = layout.clone();
        let mut depth = 0usize;
        let mut cur: &Layout = &cloned;
        loop {
            match cur {
                Layout::Nest(inner) => {
                    depth += 1;
                    cur = inner;
                }
                Layout::Text(data) => {
                    assert_eq!(data, "x");
                    break;
                }
                _ => panic!("unexpected node"),
            }
        }
        assert_eq!(depth, DEEP);
        // Both `layout` and `cloned` drop here via the iterative Drop.
    }

    #[test]
    fn clone_and_debug_match_expected() {
        let layout = Layout::Comp(
            Box::new(Layout::Text("a".to_string())),
            Box::new(Layout::Grp(Box::new(Layout::Line(
                Box::new(Layout::Text("b".to_string())),
                Box::new(Layout::Null),
            )))),
            Attr {
                pad: true,
                fix: false,
            },
        );
        let expected =
            "Comp(Text(\"a\"), Grp(Line(Text(\"b\"), Null)), Attr { pad: true, fix: false })";
        assert_eq!(format!("{:?}", layout), expected);
        assert_eq!(format!("{:?}", layout.clone()), expected);
    }

    #[test]
    fn deep_debug_is_iterative() {
        let layout = deep_nest(DEEP);
        let s = format!("{:?}", *layout);
        assert!(s.starts_with("Nest("));
        assert_eq!(s.matches("Nest(").count(), DEEP);
        assert!(s.contains("Text(\"x\")"));
    }
}
