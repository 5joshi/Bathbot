use crate::util::{AsOption, LitExt};

use proc_macro2::Span;
use syn::parse::{Error, Result};
use syn::spanned::Spanned;
use syn::{Attribute, Ident, Lit, LitStr, Meta, NestedMeta, Path};

#[derive(Debug)]
pub struct Values {
    pub name: Ident,
    pub literals: Vec<Lit>,
    pub span: Span,
}

impl Values {
    #[inline]
    pub fn new(name: Ident, literals: Vec<Lit>, span: Span) -> Self {
        Values {
            name,
            literals,
            span,
        }
    }
}

fn to_ident(p: &Path) -> Result<Ident> {
    let err_msg = if p.segments.is_empty() {
        Some("cannot convert an empty path to an identifier")
    } else if p.segments.len() > 1 {
        Some("the path must not have more than one segment")
    } else if !p.segments[0].arguments.is_empty() {
        Some("the singular path segment must not have any arguments")
    } else {
        None
    };
    if let Some(err_msg) = err_msg {
        Err(Error::new(p.span(), err_msg))
    } else {
        Ok(p.segments[0].ident.clone())
    }
}

pub fn parse_values(attr: &Attribute) -> Result<Values> {
    let meta = attr.parse_meta()?;
    match meta {
        Meta::Path(_) | Meta::NameValue(_) => {
            return Err(Error::new(
                attr.span(),
                format_args!(
                    "expected attribute of the form `#[{}(...)]`",
                    to_ident(meta.path())?
                ),
            ));
        }
        Meta::List(meta) => {
            let name = to_ident(&meta.path)?;
            let mut lits = Vec::with_capacity(meta.nested.len());
            for meta in meta.nested {
                match meta {
                    NestedMeta::Lit(l) => lits.push(l),
                    NestedMeta::Meta(m) => match m {
                        Meta::Path(path) => {
                            let i = to_ident(&path)?;
                            lits.push(Lit::Str(LitStr::new(&i.to_string(), i.span())))
                        }
                        Meta::List(_) | Meta::NameValue(_) => {
                            return Err(Error::new(
                                attr.span(),
                                "require literal or identifier at this level, not list",
                            ))
                        }
                    },
                }
            }
            Ok(Values::new(name, lits, attr.span()))
        }
    }
}

#[inline]
pub fn parse<T: AttributeOption>(values: Values) -> Result<T> {
    T::parse(values)
}

pub trait AttributeOption: Sized {
    fn parse(values: Values) -> Result<Self>;
}

impl AttributeOption for Vec<String> {
    fn parse(values: Values) -> Result<Self> {
        Ok(values
            .literals
            .into_iter()
            .map(|lit| lit.to_str())
            .collect())
    }
}

impl AttributeOption for Option<String> {
    fn parse(values: Values) -> Result<Self> {
        Ok(if values.literals.is_empty() {
            Some(String::new())
        } else if let Lit::Bool(b) = &values.literals[0] {
            if b.value {
                Some(String::new())
            } else {
                None
            }
        } else {
            let s = values.literals[0].to_str();
            match s.as_str() {
                "true" => Some(String::new()),
                "false" => None,
                _ => Some(s),
            }
        })
    }
}

impl AttributeOption for Vec<Ident> {
    #[inline]
    fn parse(values: Values) -> Result<Self> {
        Ok(values.literals.into_iter().map(|l| l.to_ident()).collect())
    }
}

impl AttributeOption for String {
    #[inline]
    fn parse(values: Values) -> Result<Self> {
        Ok(values.literals[0].to_str())
    }
}

impl<T: AttributeOption> AttributeOption for AsOption<T> {
    #[inline]
    fn parse(values: Values) -> Result<Self> {
        Ok(AsOption(Some(T::parse(values)?)))
    }
}