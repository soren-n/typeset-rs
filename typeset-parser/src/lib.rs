use proc_macro::TokenStream;
use proc_macro2::TokenStream as Quoted;
use quote::quote;
use std::ops::ControlFlow;
use syn::{
    Error, Ident, LitStr, parenthesized,
    parse::{Parse, ParseStream, Result, discouraged::Speculative},
    parse_macro_input,
};

fn parsed<T: Parse>(input: ParseStream) -> Result<T> {
    let _input = input.fork();
    match _input.parse::<T>() {
        Err(error) => Err(error),
        Ok(result) => {
            input.advance_to(&_input);
            Ok(result)
        }
    }
}

fn parse_any<T>(input: ParseStream, parsers: Vec<fn(ParseStream) -> Result<T>>) -> Result<T> {
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

fn parse_group<T>(input: ParseStream, parser: fn(ParseStream) -> Result<T>) -> Result<T> {
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

fn parse_unary_op(input: ParseStream) -> Result<UnaryOp> {
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

fn parse_binary_op(input: ParseStream) -> Result<BinaryOp> {
    use binary_tokens::*;
    parse_any(
        input,
        vec![
            |input| parsed::<Unpadded>(input).map(|_| BinaryOp::Unpadded),
            |input| parsed::<Padded>(input).map(|_| BinaryOp::Padded),
            |input| parsed::<FixedUnpadded>(input).map(|_| BinaryOp::FixedUnpadded),
            |input| parsed::<FixedPadded>(input).map(|_| BinaryOp::FixedPadded),
            |input| parsed::<DoubleNewline>(input).map(|_| BinaryOp::DoubleNewline),
            |input| parsed::<Newline>(input).map(|_| BinaryOp::Newline),
        ],
    )
}

#[derive(Debug, Clone)]
enum Ast {
    Null,
    Variable(Ident),
    Text(String),
    Unary(UnaryOp, Box<Ast>),
    Binary(BinaryOp, Box<Ast>, Box<Ast>),
}

fn parse_null(input: ParseStream) -> Result<Box<Ast>> {
    let item: Ident = input.parse()?;
    match item.to_string().as_str() {
        "null" => Ok(Box::new(Ast::Null)),
        _ => Err(Error::new(item.span(), "Expected null")),
    }
}

fn parse_variable(input: ParseStream) -> Result<Box<Ast>> {
    let name = parsed::<Ident>(input)?;
    Ok(Box::new(Ast::Variable(name)))
}

fn parse_text(input: ParseStream) -> Result<Box<Ast>> {
    let data = parsed::<LitStr>(input)?;
    Ok(Box::new(Ast::Text(data.value())))
}

fn parse_group_ast(input: ParseStream) -> Result<Box<Ast>> {
    parse_group(input, parse_ast)
}

fn parse_primary(input: ParseStream) -> Result<Box<Ast>> {
    parse_any(
        input,
        vec![parse_null, parse_variable, parse_text, parse_group_ast],
    )
}

fn parse_atom(input: ParseStream) -> Result<Box<Ast>> {
    parse_any(input, vec![parse_unary, parse_primary])
}

fn parse_unary(input: ParseStream) -> Result<Box<Ast>> {
    let op = parse_unary_op(input)?;
    let ast = parse_primary(input)?;
    Ok(Box::new(Ast::Unary(op, ast)))
}

fn parse_binary(input: ParseStream) -> Result<Box<Ast>> {
    let left = parse_atom(input)?;
    let op = parse_binary_op(input)?;
    let right = parse_ast(input)?;
    Ok(Box::new(Ast::Binary(op, left, right)))
}

fn parse_ast(input: ParseStream) -> Result<Box<Ast>> {
    parse_any(input, vec![parse_binary, parse_atom])
}

impl Parse for Box<Ast> {
    fn parse(input: ParseStream) -> Result<Self> {
        let _input = input.fork();
        match parse_ast(&_input) {
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

fn reify_layout(ast: Ast) -> Quoted {
    match ast {
        Ast::Null => quote! { typeset::null() },
        Ast::Variable(name) => quote! { #name.clone() },
        Ast::Text(data) => quote! { typeset::text(#data.to_string()) },
        Ast::Unary(UnaryOp::Fix, ast1) => {
            let layout = reify_layout(*ast1);
            quote! { typeset::fix(#layout) }
        }
        Ast::Unary(UnaryOp::Grp, ast1) => {
            let layout = reify_layout(*ast1);
            quote! { typeset::grp(#layout) }
        }
        Ast::Unary(UnaryOp::Seq, ast1) => {
            let layout = reify_layout(*ast1);
            quote! { typeset::seq(#layout) }
        }
        Ast::Unary(UnaryOp::Nest, ast1) => {
            let layout = reify_layout(*ast1);
            quote! { typeset::nest(#layout) }
        }
        Ast::Unary(UnaryOp::Pack, ast1) => {
            let layout = reify_layout(*ast1);
            quote! { typeset::pack(#layout) }
        }
        Ast::Binary(BinaryOp::Unpadded, left, right) => {
            let left_layout = reify_layout(*left);
            let right_layout = reify_layout(*right);
            quote! { typeset::unpad(#left_layout, #right_layout) }
        }
        Ast::Binary(BinaryOp::Padded, left, right) => {
            let left_layout = reify_layout(*left);
            let right_layout = reify_layout(*right);
            quote! { typeset::pad(#left_layout, #right_layout) }
        }
        Ast::Binary(BinaryOp::FixedUnpadded, left, right) => {
            let left_layout = reify_layout(*left);
            let right_layout = reify_layout(*right);
            quote! { typeset::fix_unpad(#left_layout, #right_layout) }
        }
        Ast::Binary(BinaryOp::FixedPadded, left, right) => {
            let left_layout = reify_layout(*left);
            let right_layout = reify_layout(*right);
            quote! { typeset::fix_pad(#left_layout, #right_layout) }
        }
        Ast::Binary(BinaryOp::Newline, left, right) => {
            let left_layout = reify_layout(*left);
            let right_layout = reify_layout(*right);
            quote! {
              typeset::line(
                #left_layout,
                #right_layout
              )
            }
        }
        Ast::Binary(BinaryOp::DoubleNewline, left, right) => {
            let left_layout = reify_layout(*left);
            let right_layout = reify_layout(*right);
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
    let ast = parse_macro_input!(input as Box<Ast>);
    let output = reify_layout(*ast);
    quote! { #output }.into()
}
