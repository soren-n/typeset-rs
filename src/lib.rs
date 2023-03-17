#![feature(box_patterns)]
#![allow(dead_code)]

mod util;
mod order;
mod list;
mod avl;
mod map;
mod parser;
mod compiler;

pub use self::parser::_parse;
pub use self::compiler::{
  Layout,
  Doc,
  null,
  text,
  fix,
  grp,
  seq,
  nest,
  pack,
  line,
  comp,
  compile,
  render
};