#![feature(proc_macro_diagnostic)]

use proc_macro::TokenStream;
use proc_macro2::TokenStream as Quoted;
use quote::quote;
use std::fmt::Debug;
use std::ops::ControlFlow;
use syn::{
    parenthesized,
    parse::{discouraged::Speculative, Parse, ParseStream, Result},
    parse_macro_input, Error, Ident, LitStr,
};

fn _parsed<T: Parse>(input: ParseStream) -> Result<T> {
    let _input = input.fork();
    match _input.parse::<T>() {
        Err(error) => Err(error),
        Ok(result) => {
            input.advance_to(&_input);
            Ok(result)
        }
    }
}

fn _parse_any<T>(input: ParseStream, parsers: Vec<fn(ParseStream) -> Result<T>>) -> Result<T> {
    let result = parsers.iter().try_fold(Vec::new(), |mut errors, parser| {
        let _input = input.fork();
        match parser(&_input) {
            Ok(value) => {
                input.advance_to(&_input);
                ControlFlow::Break(value)
            }
            Err(new_error) => {
                errors.push(new_error);
                ControlFlow::Continue(errors)
            }
        }
    });
    match result {
        ControlFlow::Break(value) => Ok(value),
        ControlFlow::Continue(errors) => {
            let error = Error::new(input.span(), "Failed to parse any");
            Err(errors
                .iter()
                .cloned()
                .fold(error, |mut out_error, in_error| {
                    out_error.combine(in_error);
                    out_error
                }))
        }
    }
}

fn _parse_group<T>(input: ParseStream, parser: fn(ParseStream) -> Result<T>) -> Result<T> {
    let content;
    parenthesized!(content in input);
    parser(&content)
}

#[derive(Debug, Clone)]
enum UnaryOp {
    Fix,
    Grp,
    Seq,
    Nest,
    Pack,
}

fn _parse_unary_op(input: ParseStream) -> Result<UnaryOp> {
    let item: Ident = input.parse()?;
    match item.to_string().as_str() {
        "fix" => Ok(UnaryOp::Fix),
        "grp" => Ok(UnaryOp::Grp),
        "seq" => Ok(UnaryOp::Seq),
        "nest" => Ok(UnaryOp::Nest),
        "pack" => Ok(UnaryOp::Pack),
        _ => Err(Error::new(item.span(), "Expected a unary operator")),
    }
}

mod binary_tokens {
    use syn::custom_punctuation;
    custom_punctuation!(Unpadded, &);
    custom_punctuation!(Padded, +);
    custom_punctuation!(FixedUnpadded, !&);
    custom_punctuation!(FixedPadded, !+);
    custom_punctuation!(Newline, @);
    custom_punctuation!(DoubleNewline, @@);
}

#[derive(Debug, Clone)]
enum BinaryOp {
    Unpadded,
    Padded,
    FixedUnpadded,
    FixedPadded,
    Newline,
    DoubleNewline,
}

fn _parse_binary_op(input: ParseStream) -> Result<BinaryOp> {
    use binary_tokens::*;
    _parse_any(
        input,
        vec![
            |input| _parsed::<Unpadded>(input).map(|_| BinaryOp::Unpadded),
            |input| _parsed::<Padded>(input).map(|_| BinaryOp::Padded),
            |input| _parsed::<FixedUnpadded>(input).map(|_| BinaryOp::FixedUnpadded),
            |input| _parsed::<FixedPadded>(input).map(|_| BinaryOp::FixedPadded),
            |input| _parsed::<DoubleNewline>(input).map(|_| BinaryOp::DoubleNewline),
            |input| _parsed::<Newline>(input).map(|_| BinaryOp::Newline),
        ],
    )
}

#[derive(Debug, Clone)]
enum AST {
    Null,
    Variable(Ident),
    Text(String),
    Unary(UnaryOp, Box<AST>),
    Binary(BinaryOp, Box<AST>, Box<AST>),
}

fn _parse_null(input: ParseStream) -> Result<Box<AST>> {
    let item: Ident = input.parse()?;
    match item.to_string().as_str() {
        "null" => Ok(Box::new(AST::Null)),
        _ => Err(Error::new(item.span(), "Expected a unary operator")),
    }
}

fn _parse_variable(input: ParseStream) -> Result<Box<AST>> {
    let name = _parsed::<Ident>(input)?;
    Ok(Box::new(AST::Variable(name)))
}

fn _parse_text(input: ParseStream) -> Result<Box<AST>> {
    let data = _parsed::<LitStr>(input)?;
    Ok(Box::new(AST::Text(data.value())))
}

fn _parse_group_ast(input: ParseStream) -> Result<Box<AST>> {
    _parse_group(input, _parse_ast)
}

fn _parse_primary(input: ParseStream) -> Result<Box<AST>> {
    _parse_any(
        input,
        vec![_parse_null, _parse_variable, _parse_text, _parse_group_ast],
    )
}

fn _parse_atom(input: ParseStream) -> Result<Box<AST>> {
    _parse_any(input, vec![_parse_unary, _parse_primary])
}

fn _parse_unary(input: ParseStream) -> Result<Box<AST>> {
    let op = _parse_unary_op(input)?;
    let ast = _parse_primary(input)?;
    Ok(Box::new(AST::Unary(op, ast)))
}

fn _parse_binary(input: ParseStream) -> Result<Box<AST>> {
    let left = _parse_atom(input)?;
    let op = _parse_binary_op(input)?;
    let right = _parse_ast(input)?;
    Ok(Box::new(AST::Binary(op, left, right)))
}

fn _parse_ast(input: ParseStream) -> Result<Box<AST>> {
    _parse_any(input, vec![_parse_binary, _parse_atom])
}

impl Parse for Box<AST> {
    fn parse(input: ParseStream) -> Result<Self> {
        let _input = input.fork();
        match _parse_ast(&_input) {
            Err(error) => Err(error),
            Ok(result) => {
                input.advance_to(&_input);
                if input.is_empty() {
                    Ok(result)
                } else {
                    Err(Error::new(
                        input.span(),
                        format!("Failed to parse layout:\n{}", input),
                    ))
                }
            }
        }
    }
}

fn _reify_layout(ast: Box<AST>) -> Quoted {
    match *ast {
        AST::Null => quote! { typeset::null() },
        AST::Variable(name) => quote! { #name.clone() },
        AST::Text(data) => quote! { typeset::text(#data.to_string()) },
        AST::Unary(UnaryOp::Fix, ast1) => {
            let layout = _reify_layout(ast1);
            quote! { typeset::fix(#layout) }
        }
        AST::Unary(UnaryOp::Grp, ast1) => {
            let layout = _reify_layout(ast1);
            quote! { typeset::grp(#layout) }
        }
        AST::Unary(UnaryOp::Seq, ast1) => {
            let layout = _reify_layout(ast1);
            quote! { typeset::seq(#layout) }
        }
        AST::Unary(UnaryOp::Nest, ast1) => {
            let layout = _reify_layout(ast1);
            quote! { typeset::nest(#layout) }
        }
        AST::Unary(UnaryOp::Pack, ast1) => {
            let layout = _reify_layout(ast1);
            quote! { typeset::pack(#layout) }
        }
        AST::Binary(BinaryOp::Unpadded, left, right) => {
            let left_layout = _reify_layout(left);
            let right_layout = _reify_layout(right);
            quote! {
              typeset::comp(
                #left_layout,
                #right_layout,
                false,
                false
              )
            }
        }
        AST::Binary(BinaryOp::Padded, left, right) => {
            let left_layout = _reify_layout(left);
            let right_layout = _reify_layout(right);
            quote! {
              typeset::comp(
                #left_layout,
                #right_layout,
                true,
                false
              )
            }
        }
        AST::Binary(BinaryOp::FixedUnpadded, left, right) => {
            let left_layout = _reify_layout(left);
            let right_layout = _reify_layout(right);
            quote! {
              typeset::comp(
                #left_layout,
                #right_layout,
                false,
                true
              )
            }
        }
        AST::Binary(BinaryOp::FixedPadded, left, right) => {
            let left_layout = _reify_layout(left);
            let right_layout = _reify_layout(right);
            quote! {
              typeset::comp(
                #left_layout,
                #right_layout,
                true,
                true
              )
            }
        }
        AST::Binary(BinaryOp::Newline, left, right) => {
            let left_layout = _reify_layout(left);
            let right_layout = _reify_layout(right);
            quote! {
              typeset::line(
                #left_layout,
                #right_layout
              )
            }
        }
        AST::Binary(BinaryOp::DoubleNewline, left, right) => {
            let left_layout = _reify_layout(left);
            let right_layout = _reify_layout(right);
            quote! {
              typeset::line(
                #left_layout,
                typeset::line(
                  typeset::null(),
                  #right_layout
                )
              )
            }
        }
    }
}

#[proc_macro]
pub fn layout(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as Box<AST>);
    let output = _reify_layout(ast);
    quote! { #output }.into()
}
