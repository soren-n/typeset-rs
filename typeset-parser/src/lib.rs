#![doc = include_str!("../README.md")]

use proc_macro2::{Delimiter, Ident, Span, TokenStream, TokenTree};
use quote::{quote, quote_spanned};
use typeset::dsl::{self, Binary, Build, ParseErrorAt, Token, Unary};
use typeset::{Break, Pad};

/// A parse failure at the span of the offending token.
type Error = ParseErrorAt<Span>;

fn compile_error(error: Error) -> TokenStream {
    let message = error.message;
    quote_spanned! {error.at=> ::core::compile_error!(#message) }
}

/// Flattens the macro input into DSL tokens with their spans. Parenthesized
/// groups become `(` … `)`; multi-character operators are recognized from
/// jointly spaced punctuation, so `!&` and `@@` arrive whole.
fn tokenize(input: TokenStream) -> Result<Vec<(Span, Token<Ident>)>, Error> {
    let mut tokens = Vec::new();
    // A stack of the token streams being flattened, each with its group's
    // span, so group nesting costs heap rather than native stack.
    let mut streams = vec![(Span::call_site(), input.into_iter().peekable())];
    while let Some((_, stream)) = streams.last_mut() {
        let Some(tree) = stream.next() else {
            let (span, _) = streams.pop().expect("a stream is open");
            if !streams.is_empty() {
                tokens.push((span, Token::Close));
            }
            continue;
        };
        let span = tree.span();
        let token = match tree {
            TokenTree::Group(group) => {
                if group.delimiter() != Delimiter::Parenthesis {
                    return Err(Error {
                        at: span,
                        message: "expected parentheses",
                    });
                }
                tokens.push((span, Token::Open));
                streams.push((span, group.stream().into_iter().peekable()));
                continue;
            }
            // The literal's source text, escapes as written, read by the
            // DSL's own string syntax.
            TokenTree::Literal(literal) => match dsl::parse_text(&literal.to_string()) {
                Ok(text) => Token::Text(text),
                Err(e) => {
                    return Err(Error {
                        at: span,
                        message: e.message,
                    });
                }
            },
            TokenTree::Ident(ident) => match ident.to_string().as_str() {
                "null" => Token::Null,
                name => match Unary::from_keyword(name) {
                    Some(unary) => Token::Unary(unary),
                    None => Token::Var(ident),
                },
            },
            TokenTree::Punct(punct) => {
                let mut op = punct.as_char().to_string();
                if punct.spacing() == proc_macro2::Spacing::Joint
                    && let Some(TokenTree::Punct(next)) = stream.peek()
                    && Binary::from_symbol(&format!("{op}{}", next.as_char())).is_some()
                {
                    op.push(next.as_char());
                    stream.next();
                }
                match Binary::from_symbol(&op) {
                    Some(op) => Token::Binary(op),
                    None => {
                        return Err(Error {
                            at: span,
                            message: "expected an operator",
                        });
                    }
                }
            }
        };
        tokens.push((span, token));
    }
    Ok(tokens)
}

/// Builds constructor calls.
struct Reify;

impl Build for Reify {
    type Var = Ident;
    type Out = TokenStream;
    fn null(&mut self) -> TokenStream {
        quote! { typeset::null() }
    }
    fn text(&mut self, data: String) -> TokenStream {
        quote! { typeset::text(#data) }
    }
    fn var(&mut self, var: Ident) -> TokenStream {
        quote! { #var.clone() }
    }
    fn unary(&mut self, op: Unary, layout: TokenStream) -> TokenStream {
        match op {
            Unary::Fix => quote! { typeset::fix(#layout) },
            Unary::Grp => quote! { typeset::grp(#layout) },
            Unary::Seq => quote! { typeset::seq(#layout) },
            Unary::Nest => quote! { typeset::nest(#layout) },
            Unary::Pack => quote! { typeset::pack(#layout) },
        }
    }
    fn binary(&mut self, op: Binary, left: TokenStream, right: TokenStream) -> TokenStream {
        match op {
            Binary::Line => quote! { typeset::line(#left, #right) },
            Binary::BlankLine => {
                quote! { typeset::line(#left, typeset::line(typeset::null(), #right)) }
            }
            Binary::Comp(Pad::Padded, Break::Breakable) => quote! { typeset::pad(#left, #right) },
            Binary::Comp(Pad::Unpadded, Break::Breakable) => {
                quote! { typeset::unpad(#left, #right) }
            }
            Binary::Comp(Pad::Padded, Break::Fixed) => quote! { typeset::fix_pad(#left, #right) },
            Binary::Comp(Pad::Unpadded, Break::Fixed) => {
                quote! { typeset::fix_unpad(#left, #right) }
            }
        }
    }
}

fn expand(input: TokenStream) -> Result<TokenStream, Error> {
    let tokens = tokenize(input)?;
    let end = tokens.last().map_or(Span::call_site(), |(span, _)| *span);
    dsl::parse_tokens(tokens, end, &mut Reify)
}

/// Builds a [`typeset::Layout`] from the layout DSL.
#[proc_macro]
pub fn layout(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    expand(input.into()).unwrap_or_else(compile_error).into()
}
