#![feature(box_patterns)]
#![allow(dead_code)]

mod avl;
mod compiler;
mod list;
mod map;
mod order;
mod util;

pub use self::compiler::{
    comp, compile, fix, grp, line, nest, null, pack, render, seq, text, Doc, Layout,
};
