use std::fmt;

/// Attribute structure for Layout compositions
#[derive(Debug, Copy, Clone)]
pub struct Attr {
    pub pad: bool,
    pub fix: bool,
}

/// Layout AST - the input language for the compiler
#[derive(Debug, Clone, Default)]
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

impl fmt::Display for Layout {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        #[allow(clippy::boxed_local)]
        fn _visit(layout: Box<Layout>) -> String {
            match *layout {
                Layout::Null => "Null".to_string(),
                Layout::Text(data) => format!("(Text \"{}\")", data),
                Layout::Fix(layout1) => {
                    let layout_s = _visit(layout1);
                    format!("(Fix {})", layout_s)
                }
                Layout::Grp(layout1) => {
                    let layout_s = _visit(layout1);
                    format!("(Grp {})", layout_s)
                }
                Layout::Seq(layout1) => {
                    let layout_s = _visit(layout1);
                    format!("(Seq {})", layout_s)
                }
                Layout::Nest(layout1) => {
                    let layout_s = _visit(layout1);
                    format!("(Nest {})", layout_s)
                }
                Layout::Pack(layout1) => {
                    let layout_s = _visit(layout1);
                    format!("(Pack {})", layout_s)
                }
                Layout::Line(left, right) => {
                    let left_s = _visit(left);
                    let right_s = _visit(right);
                    format!("(Line {} {})", left_s, right_s)
                }
                Layout::Comp(left, right, attr) => {
                    let left_s = _visit(left);
                    let right_s = _visit(right);
                    format!("(Comp {} {} {} {})", left_s, right_s, attr.pad, attr.fix)
                }
            }
        }
        write!(f, "{}", _visit(Box::new(self.clone())))
    }
}
