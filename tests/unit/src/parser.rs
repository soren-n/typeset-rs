use pest::Parser;
use pest_derive::Parser;
use pest::iterators::Pairs;
use pest::pratt_parser::PrattParser;

use typeset::{
  Layout,
  null,
  text,
  fix,
  grp,
  seq,
  nest,
  pack,
  line,
  comp
};

#[derive(Parser)]
#[grammar = "layout.pest"]
pub struct LayoutParser;

lazy_static::lazy_static! {
  static ref PRATT_PARSER: PrattParser<Rule> = {
    use pest::pratt_parser::{Assoc::*, Op};
    PrattParser::new()
      .op(
        Op::infix(Rule::single_line_op, Right) |
        Op::infix(Rule::double_line_op, Right) |
        Op::infix(Rule::unpad_comp_op, Right) |
        Op::infix(Rule::pad_comp_op, Right) |
        Op::infix(Rule::fix_unpad_comp_op, Right) |
        Op::infix(Rule::fix_pad_comp_op, Right)
      )
      .op(
        Op::prefix(Rule::fix_op) |
        Op::prefix(Rule::grp_op) |
        Op::prefix(Rule::seq_op) |
        Op::prefix(Rule::nest_op) |
        Op::prefix(Rule::pack_op)
      )
  };
}

#[derive(Debug)]
enum Syntax {
  Null,
  Index(usize),
  Text(String),
  Fix(Box<Syntax>),
  Grp(Box<Syntax>),
  Seq(Box<Syntax>),
  Nest(Box<Syntax>),
  Pack(Box<Syntax>),
  SingleLine(Box<Syntax>, Box<Syntax>),
  DoubleLine(Box<Syntax>, Box<Syntax>),
  UnpadComp(Box<Syntax>, Box<Syntax>),
  PadComp(Box<Syntax>, Box<Syntax>),
  FixUnpadComp(Box<Syntax>, Box<Syntax>),
  FixPadComp(Box<Syntax>, Box<Syntax>)
}

#[doc(hidden)]
pub fn parse(
  input: &str,
  args: &Vec<Box<Layout>>
) -> Result<Box<Layout>, String> {
  fn _parse_syntax(tokens: Pairs<Rule>) -> Result<Box<Syntax>, String> {
    PRATT_PARSER
      .map_primary(|primary| match primary.as_rule() {
        Rule::null =>
          Ok(Box::new(Syntax::Null)),
        Rule::index =>
          Ok(Box::new(Syntax::Index(primary.as_str().parse::<usize>().unwrap()))),
        Rule::text => {
          primary.into_inner().try_fold(
            String::new(),
            |mut result, part| match part.as_rule() {
              Rule::raw_string => { result.push_str(part.as_str()); Ok(result) }
              Rule::escaped_string => match &part.as_str()[1..] {
                "n" => { result.push('\n'); Ok(result) }
                "r" => { result.push('\r'); Ok(result) }
                "t" => { result.push('\t'); Ok(result) }
                "\\" => { result.push('\\'); Ok(result) }
                "0" => { result.push('\0'); Ok(result) }
                "\"" => { result.push('\"'); Ok(result) }
                "'" => { result.push('\''); Ok(result) }
                char => Err(format!("Unexpected escaped character: \\{char:?}"))
              },
              _ => Err(format!("Unexpected token: {part:?}"))
            }
          ).map(|result| Box::new(Syntax::Text(result)))
        }
        Rule::expr =>
          _parse_syntax(primary.into_inner()),
        rule =>
          Err(format!("expected atom, found {:?}", rule))
      })
      .map_infix(|left, op, right| match op.as_rule() {
        Rule::single_line_op =>
          Ok(Box::new(Syntax::SingleLine(left?, right?))),
        Rule::double_line_op =>
          Ok(Box::new(Syntax::DoubleLine(left?, right?))),
        Rule::unpad_comp_op =>
          Ok(Box::new(Syntax::UnpadComp(left?, right?))),
        Rule::pad_comp_op =>
          Ok(Box::new(Syntax::PadComp(left?, right?))),
        Rule::fix_unpad_comp_op =>
          Ok(Box::new(Syntax::FixUnpadComp(left?, right?))),
        Rule::fix_pad_comp_op =>
          Ok(Box::new(Syntax::FixPadComp(left?, right?))),
        rule =>
          Err(format!("expected binary operator, found {:?}", rule))
      })
      .map_prefix(|op, syntax| match op.as_rule() {
        Rule::fix_op => Ok(Box::new(Syntax::Fix(syntax?))),
        Rule::grp_op => Ok(Box::new(Syntax::Grp(syntax?))),
        Rule::seq_op => Ok(Box::new(Syntax::Seq(syntax?))),
        Rule::nest_op => Ok(Box::new(Syntax::Nest(syntax?))),
        Rule::pack_op => Ok(Box::new(Syntax::Pack(syntax?))),
        rule =>
          Err(format!("expected unary operator, found {:?}", rule))
      })
      .parse(tokens)
  }
  fn _interp_syntax(
    syntax: Box<Syntax>,
    args: &Vec<Box<Layout>>
  ) -> Result<Box<Layout>, String> {
    match syntax {
      box Syntax::Null =>
        Ok(null()),
      box Syntax::Index(index) => {
        let length = args.len();
        if index < length { Ok(args[index].clone()) } else {
        Err(format!("invalid index {:?}", index)) }
      }
      box Syntax::Text(data) =>
        Ok(text(data)),
      box Syntax::Fix(syntax1) => {
        let layout = _interp_syntax(syntax1, args);
        Ok(fix(layout?))
      }
      box Syntax::Grp(syntax1) => {
        let layout = _interp_syntax(syntax1, args);
        Ok(grp(layout?))
      }
      box Syntax::Seq(syntax1) => {
        let layout = _interp_syntax(syntax1, args);
        Ok(seq(layout?))
      }
      box Syntax::Nest(syntax1) => {
        let layout = _interp_syntax(syntax1, args);
        Ok(nest(layout?))
      }
      box Syntax::Pack(syntax1) => {
        let layout = _interp_syntax(syntax1, args);
        Ok(pack(layout?))
      }
      box Syntax::SingleLine(left, right) => {
        let left1 = _interp_syntax(left, args);
        let right1 = _interp_syntax(right, args);
        Ok(line(left1?, right1?))
      }
      box Syntax::DoubleLine(left, right) => {
        let left1 = _interp_syntax(left, args);
        let right1 = _interp_syntax(right, args);
        Ok(line(left1?, line(null(), right1?)))
      }
      box Syntax::UnpadComp(left, right) => {
        let left1 = _interp_syntax(left, args);
        let right1 = _interp_syntax(right, args);
        Ok(comp(left1?, right1?, false, false))
      }
      box Syntax::PadComp(left, right) => {
        let left1 = _interp_syntax(left, args);
        let right1 = _interp_syntax(right, args);
        Ok(comp(left1?, right1?, true, false))
      }
      box Syntax::FixUnpadComp(left, right) => {
        let left1 = _interp_syntax(left, args);
        let right1 = _interp_syntax(right, args);
        Ok(comp(left1?, right1?, false, true))
      }
      box Syntax::FixPadComp(left, right) => {
        let left1 = _interp_syntax(left, args);
        let right1 = _interp_syntax(right, args);
        Ok(comp(left1?, right1?, true, true))
      }
    }
  }
  match LayoutParser::parse(Rule::layout, &input) {
    Ok(mut tokens) =>
      _interp_syntax(
        _parse_syntax(tokens.next().unwrap().into_inner())?,
        args
      ),
    Err(error) => Err(format!("{}", error))
  }
}