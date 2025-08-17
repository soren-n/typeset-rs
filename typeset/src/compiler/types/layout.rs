use std::fmt;

/// Attribute structure for Layout compositions
#[derive(Debug, Copy, Clone)]
pub struct Attr {
    pub pad: bool,
    pub fix: bool,
}

/// Layout AST - the input language for the compiler
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
    Comp(Box<Layout>, Box<Layout>, Attr),
}

impl Default for Layout {
    fn default() -> Self {
        Layout::Null
    }
}

impl fmt::Display for Layout {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fn _visit(layout: Box<Layout>) -> String {
            match layout {
                box Layout::Null => "Null".to_string(),
                box Layout::Text(data) => format!("(Text \"{}\")", data),
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
