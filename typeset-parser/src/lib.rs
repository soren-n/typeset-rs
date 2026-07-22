use proc_macro::TokenStream;
use proc_macro2::TokenStream as Quoted;
use quote::quote;
use syn::{
    Error, Ident, LitStr, parenthesized,
    parse::{Parse, ParseStream, Result, discouraged::Speculative},
    parse_macro_input,
};

/// Speculatively parse a `T`: advance `input` only on success, so a failed
/// attempt leaves it untouched for the next alternative.
fn parsed<T: Parse>(input: ParseStream) -> Result<T> {
    let fork = input.fork();
    let result = fork.parse::<T>()?;
    input.advance_to(&fork);
    Ok(result)
}

/// Try each parser in order against a fork of `input`, committing to the first
/// that succeeds. If none does, the error combines every alternative's error.
fn parse_any<T>(input: ParseStream, parsers: &[fn(ParseStream) -> Result<T>]) -> Result<T> {
    let mut errors: Vec<Error> = Vec::new();
    for parser in parsers {
        let fork = input.fork();
        match parser(&fork) {
            Ok(value) => {
                input.advance_to(&fork);
                return Ok(value);
            }
            Err(error) => errors.push(error),
        }
    }
    let mut combined = Error::new(input.span(), "Failed to parse any");
    for error in errors {
        combined.combine(error);
    }
    Err(combined)
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
        &[
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
        &[parse_null, parse_variable, parse_text, parse_group_ast],
    )
}

fn parse_atom(input: ParseStream) -> Result<Box<Ast>> {
    parse_any(input, &[parse_unary, parse_primary])
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
    parse_any(input, &[parse_binary, parse_atom])
}

impl Parse for Box<Ast> {
    fn parse(input: ParseStream) -> Result<Self> {
        let fork = input.fork();
        let result = parse_ast(&fork)?;
        input.advance_to(&fork);
        if !input.is_empty() {
            return Err(Error::new(
                input.span(),
                format!("Failed to parse layout:\n{}", input),
            ));
        }
        Ok(result)
    }
}

impl UnaryOp {
    /// The `typeset` constructor call this operator reifies to.
    fn reify(&self, layout: Quoted) -> Quoted {
        match self {
            UnaryOp::Fix => quote! { typeset::fix(#layout) },
            UnaryOp::Grp => quote! { typeset::grp(#layout) },
            UnaryOp::Seq => quote! { typeset::seq(#layout) },
            UnaryOp::Nest => quote! { typeset::nest(#layout) },
            UnaryOp::Pack => quote! { typeset::pack(#layout) },
        }
    }
}

impl BinaryOp {
    /// The `typeset` constructor call this operator reifies to.
    fn reify(&self, left: Quoted, right: Quoted) -> Quoted {
        match self {
            BinaryOp::Unpadded => quote! { typeset::unpad(#left, #right) },
            BinaryOp::Padded => quote! { typeset::pad(#left, #right) },
            BinaryOp::FixedUnpadded => quote! { typeset::fix_unpad(#left, #right) },
            BinaryOp::FixedPadded => quote! { typeset::fix_pad(#left, #right) },
            BinaryOp::Newline => quote! { typeset::line(#left, #right) },
            BinaryOp::DoubleNewline => quote! {
                typeset::line(#left, typeset::line(typeset::null(), #right))
            },
        }
    }
}

fn reify_layout(ast: Ast) -> Quoted {
    match ast {
        Ast::Null => quote! { typeset::null() },
        Ast::Variable(name) => quote! { #name.clone() },
        Ast::Text(data) => quote! { typeset::text(#data) },
        Ast::Unary(op, ast1) => op.reify(reify_layout(*ast1)),
        Ast::Binary(op, left, right) => op.reify(reify_layout(*left), reify_layout(*right)),
    }
}

#[proc_macro]
pub fn layout(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as Box<Ast>);
    let output = reify_layout(*ast);
    quote! { #output }.into()
}
