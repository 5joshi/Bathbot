use crate::util::{Argument, AsOption, Parenthesised};

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{quote, ToTokens};
use syn::{
    braced,
    parse::{Error, Parse, ParseStream, Result},
    parse_quote,
    spanned::Spanned,
    Attribute, Block, FnArg, Ident, Pat, ReturnType, Stmt, Token, Type, Visibility,
};

pub struct CommandFun {
    // #[...]
    pub attributes: Vec<Attribute>,
    // pub / nothing
    visibility: Visibility,
    // name
    pub name: Ident,
    // (...)
    pub args: Vec<Argument>,
    // -> ...
    pub ret: Type,
    // { ... }
    pub body: Vec<Stmt>,
}

impl Parse for CommandFun {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        // #[...]
        let attributes = input.call(Attribute::parse_outer)?;

        // pub / nothing
        let visibility = input.parse::<Visibility>()?;

        // async fn
        input.parse::<Token![async]>()?;
        input.parse::<Token![fn]>()?;

        // name
        let name = input.parse::<Ident>()?;

        // arguments
        let Parenthesised(args) = input.parse::<Parenthesised<FnArg>>()?;

        let mut args: Vec<_> = args
            .into_iter()
            .map(parse_argument)
            .collect::<Result<_>>()?;

        let mut iter = args.iter_mut();

        match iter.next() {
            Some(arg) if arg.kind == parse_quote! { Arc<Context> } => {}
            Some(arg) => {
                return Err(Error::new(
                    arg.kind.span(),
                    "expected first argument of type `Arc<Context>`",
                ));
            }
            None => {
                return Err(Error::new(
                    Span::call_site(),
                    "expected first argument of type `Arc<Context>`",
                ));
            }
        }

        match iter.next() {
            Some(arg) if arg.kind == parse_quote! { CommandData } => {
                arg.kind = parse_quote! { CommandData<'fut> };
            }
            Some(arg) => {
                return Err(Error::new(
                    arg.kind.span(),
                    "expected second argument of type `CommandData`",
                ));
            }
            None => {
                return Err(Error::new(
                    Span::call_site(),
                    "expected second argument of type `CommandData`",
                ));
            }
        }

        if let Some(next) = iter.next() {
            return Err(Error::new(next.span(), "expected only two arguments"));
        }

        // -> BotResult<()>
        let ret = match input.parse::<ReturnType>()? {
            ReturnType::Type(_, t) => {
                if t == parse_quote! { BotResult<()> } {
                    *t
                } else {
                    return Err(input.error("expected return type `BotResult<()>`"));
                }
            }
            ReturnType::Default => return Err(input.error("expected a return value")),
        };

        // { ... }
        let body_content;
        braced!(body_content in input);
        let body = body_content.call(Block::parse_within)?;

        Ok(Self {
            attributes,
            visibility,
            name,
            args,
            ret,
            body,
        })
    }
}

impl ToTokens for CommandFun {
    fn to_tokens(&self, stream: &mut TokenStream2) {
        let Self {
            attributes: _,
            visibility,
            name,
            args,
            ret,
            body,
        } = self;

        stream.extend(quote! {
            #visibility async fn #name<'fut>(#(#args),*) -> #ret {
                #(#body)*
            }
        });
    }
}

fn parse_argument(arg: FnArg) -> Result<Argument> {
    match arg {
        FnArg::Typed(typed) => {
            let pat = typed.pat;
            match *pat {
                Pat::Ident(id) => {
                    let name = id.ident;
                    let mutable = id.mutability;

                    Ok(Argument {
                        mutable,
                        name,
                        kind: *typed.ty,
                    })
                }
                Pat::Wild(wild) => {
                    let token = wild.underscore_token;
                    let name = Ident::new("_", token.spans[0]);

                    Ok(Argument {
                        mutable: None,
                        name,
                        kind: *typed.ty,
                    })
                }
                _ => Err(Error::new(
                    pat.span(),
                    "expected either _ or identifier before `:`",
                )),
            }
        }
        FnArg::Receiver(_) => Err(Error::new(
            arg.span(),
            "expected arguments of the form `identifier: type`",
        )),
    }
}

#[derive(Default)]
pub struct Options {
    pub aliases: Vec<String>,
    pub short_desc: Option<String>,
    pub long_desc: AsOption<String>,
    pub usage: AsOption<String>,
    pub examples: Vec<String>,
    pub authority: bool,
    pub owner: bool,
    pub only_guilds: bool,
    pub bucket: AsOption<String>,
    pub no_typing: bool,
    pub sub_commands: Vec<Ident>,
}
