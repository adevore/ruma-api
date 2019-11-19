//! Details of the `request` section of the procedural macro.

use std::{convert::TryFrom, mem};

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned, ToTokens};
use syn::{spanned::Spanned, Field, Ident};

use crate::api::{
    attribute::{Meta, MetaNameValue},
    strip_serde_attrs, RawRequest,
};

/// The result of processing the `request` section of the macro.
pub struct Request {
    /// The fields of the request.
    fields: Vec<RequestField>,
}

impl Request {
    /// Produces code to add necessary HTTP headers to an `http::Request`.
    pub fn add_headers_to_request(&self) -> TokenStream {
        let append_stmts = self.header_fields().map(|request_field| {
            let (field, header_name) = match request_field {
                RequestField::Header(field, header_name) => (field, header_name),
                _ => unreachable!("expected request field to be header variant"),
            };

            let field_name = &field.ident;

            quote! {
                headers.append(
                    ruma_api::exports::http::header::#header_name,
                    ruma_api::exports::http::header::HeaderValue::from_str(request.#field_name.as_ref())
                        .expect("failed to convert value into HeaderValue"),
                );
            }
        });

        quote! {
            #(#append_stmts)*
        }
    }

    /// Produces code to extract fields from the HTTP headers in an `http::Request`.
    pub fn parse_headers_from_request(&self) -> TokenStream {
        let fields = self.header_fields().map(|request_field| {
            let (field, header_name) = match request_field {
                RequestField::Header(field, header_name) => (field, header_name),
                _ => panic!("expected request field to be header variant"),
            };

            let field_name = &field.ident;
            let header_name_string = header_name.to_string();

            quote! {
                #field_name: headers.get(ruma_api::exports::http::header::#header_name)
                    .and_then(|v| v.to_str().ok())
                    .ok_or(ruma_api::exports::serde_json::Error::missing_field(#header_name_string))?
                    .to_owned()
            }
        });

        quote! {
            #(#fields,)*
        }
    }

    /// Whether or not this request has any data in the HTTP body.
    pub fn has_body_fields(&self) -> bool {
        self.fields.iter().any(|field| field.is_body())
    }

    /// Whether or not this request has any data in HTTP headers.
    pub fn has_header_fields(&self) -> bool {
        self.fields.iter().any(|field| field.is_header())
    }
    /// Whether or not this request has any data in the URL path.
    pub fn has_path_fields(&self) -> bool {
        self.fields.iter().any(|field| field.is_path())
    }

    /// Whether or not this request has any data in the query string.
    pub fn has_query_fields(&self) -> bool {
        self.fields.iter().any(|field| field.is_query())
    }

    /// Produces an iterator over all the body fields.
    pub fn body_fields(&self) -> impl Iterator<Item = &Field> {
        self.fields.iter().filter_map(|field| field.as_body_field())
    }

    /// Produces an iterator over all the header fields.
    pub fn header_fields(&self) -> impl Iterator<Item = &RequestField> {
        self.fields.iter().filter(|field| field.is_header())
    }

    /// Gets the number of path fields.
    pub fn path_field_count(&self) -> usize {
        self.fields.iter().filter(|field| field.is_path()).count()
    }

    /// Gets the path field with the given name.
    pub fn path_field(&self, name: &str) -> Option<&Field> {
        self.fields
            .iter()
            .flat_map(|f| f.field_of_kind(RequestFieldKind::Path))
            .find(|field| {
                field
                    .ident
                    .as_ref()
                    .expect("expected field to have an identifier")
                    == name
            })
    }

    /// Returns the body field.
    pub fn newtype_body_field(&self) -> Option<&Field> {
        self.fields.iter().find_map(RequestField::as_newtype_body_field)
    }

    /// Produces code for a struct initializer for body fields on a variable named `request`.
    pub fn request_body_init_fields(&self) -> TokenStream {
        self.struct_init_fields(RequestFieldKind::Body, quote!(request))
    }

    /// Produces code for a struct initializer for path fields on a variable named `request`.
    pub fn request_path_init_fields(&self) -> TokenStream {
        self.struct_init_fields(RequestFieldKind::Path, quote!(request))
    }

    /// Produces code for a struct initializer for query string fields on a variable named `request`.
    pub fn request_query_init_fields(&self) -> TokenStream {
        self.struct_init_fields(RequestFieldKind::Query, quote!(request))
    }

    /// Produces code for a struct initializer for body fields on a variable named `request_body`.
    pub fn request_init_body_fields(&self) -> TokenStream {
        self.struct_init_fields(RequestFieldKind::Body, quote!(request_body))
    }

    /// Produces code for a struct initializer for query string fields on a variable named
    /// `request_query`.
    pub fn request_init_query_fields(&self) -> TokenStream {
        self.struct_init_fields(RequestFieldKind::Query, quote!(request_query))
    }

    /// Produces code for a struct initializer for the given field kind to be accessed through the
    /// given variable name.
    fn struct_init_fields(
        &self,
        request_field_kind: RequestFieldKind,
        src: TokenStream,
    ) -> TokenStream {
        let fields = self.fields.iter().filter_map(|f| {
            f.field_of_kind(request_field_kind).map(|field| {
                let field_name =
                    field.ident.as_ref().expect("expected field to have an identifier");
                let span = field.span();

                quote_spanned! {span=>
                    #field_name: #src.#field_name
                }
            })
        });

        quote! {
            #(#fields,)*
        }
    }
}

impl TryFrom<RawRequest> for Request {
    type Error = syn::Error;

    fn try_from(raw: RawRequest) -> syn::Result<Self> {
        let mut newtype_body_field = None;

        let fields = raw
            .fields
            .into_iter()
            .map(|mut field| {
                let mut field_kind = None;
                let mut header = None;

                for attr in mem::replace(&mut field.attrs, Vec::new()) {
                    let meta = match Meta::from_attribute(&attr)? {
                        Some(m) => m,
                        None => {
                            field.attrs.push(attr);
                            continue;
                        }
                    };

                    if field_kind.is_some() {
                        return Err(syn::Error::new_spanned(
                            attr,
                            "There can only be one field kind attribute",
                        ));
                    }

                    field_kind = Some(match meta {
                        Meta::Word(ident) => {
                            match &ident.to_string()[..] {
                                "body" => {
                                    if let Some(f) = &newtype_body_field {
                                        let mut error = syn::Error::new_spanned(
                                            field,
                                            "There can only be one newtype body field",
                                        );
                                        error.combine(syn::Error::new_spanned(
                                            f,
                                            "Previous newtype body field",
                                        ));
                                        return Err(error);
                                    }

                                    newtype_body_field = Some(field.clone());
                                    RequestFieldKind::NewtypeBody
                                }
                                "path" => RequestFieldKind::Path,
                                "query" => RequestFieldKind::Query,
                                _ => {
                                    return Err(syn::Error::new_spanned(
                                        ident,
                                        "Invalid #[ruma_api] argument, expected one of `body`, `path`, `query`",
                                    ));
                                }
                            }
                        }
                        Meta::NameValue(MetaNameValue { name, value }) => {
                            if name != "header" {
                                return Err(syn::Error::new_spanned(
                                    name,
                                    "Invalid #[ruma_api] argument with value, expected `header`"
                                ));
                            }

                            header = Some(value);
                            RequestFieldKind::Header
                        }
                    });
                }

                Ok(RequestField::new(
                    field_kind.unwrap_or(RequestFieldKind::Body),
                    field,
                    header,
                ))
            })
            .collect::<syn::Result<Vec<_>>>()?;

        if newtype_body_field.is_some() && fields.iter().any(|f| f.is_body()) {
            return Err(syn::Error::new_spanned(
                // TODO: raw,
                raw.request_kw,
                "Can't have both a newtype body field and regular body fields",
            ));
        }

        Ok(Self { fields })
    }
}

impl ToTokens for Request {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let request_struct_header = quote! {
            #[derive(Debug, Clone)]
            pub struct Request
        };

        let request_struct_body = if self.fields.is_empty() {
            quote!(;)
        } else {
            let fields =
                self.fields.iter().map(|request_field| strip_serde_attrs(request_field.field()));

            quote! {
                {
                    #(#fields),*
                }
            }
        };

        let request_body_struct = if let Some(field) = self.newtype_body_field() {
            let ty = &field.ty;
            let span = field.span();

            quote_spanned! {span=>
                /// Data in the request body.
                #[derive(Debug, ruma_api::exports::serde::Deserialize, ruma_api::exports::serde::Serialize)]
                struct RequestBody(#ty);
            }
        } else if self.has_body_fields() {
            let fields = self.fields.iter().filter_map(RequestField::as_body_field);

            quote! {
                /// Data in the request body.
                #[derive(Debug, ruma_api::exports::serde::Deserialize, ruma_api::exports::serde::Serialize)]
                struct RequestBody {
                    #(#fields),*
                }
            }
        } else {
            TokenStream::new()
        };

        let request_path_struct = if self.has_path_fields() {
            let fields = self.fields.iter().filter_map(RequestField::as_path_field);

            quote! {
                /// Data in the request path.
                #[derive(
                    Debug,
                    ruma_api::exports::serde::Deserialize,
                    ruma_api::exports::serde::Serialize,
                )]
                struct RequestPath {
                    #(#fields),*
                }
            }
        } else {
            TokenStream::new()
        };

        let request_query_struct = if self.has_query_fields() {
            let fields = self.fields.iter().filter_map(RequestField::as_query_field);

            quote! {
                /// Data in the request's query string.
                #[derive(
                    Debug,
                    ruma_api::exports::serde::Deserialize,
                    ruma_api::exports::serde::Serialize,
                )]
                struct RequestQuery {
                    #(#fields),*
                }
            }
        } else {
            TokenStream::new()
        };

        let request = quote! {
            #request_struct_header
            #request_struct_body
            #request_body_struct
            #request_path_struct
            #request_query_struct
        };

        request.to_tokens(tokens);
    }
}

/// The types of fields that a request can have.
pub enum RequestField {
    /// JSON data in the body of the request.
    Body(Field),
    /// Data in an HTTP header.
    Header(Field, Ident),
    /// A specific data type in the body of the request.
    NewtypeBody(Field),
    /// Data that appears in the URL path.
    Path(Field),
    /// Data that appears in the query string.
    Query(Field),
}

impl RequestField {
    /// Creates a new `RequestField`.
    fn new(kind: RequestFieldKind, field: Field, header: Option<Ident>) -> Self {
        match kind {
            RequestFieldKind::Body => RequestField::Body(field),
            RequestFieldKind::Header => {
                RequestField::Header(field, header.expect("missing header name"))
            }
            RequestFieldKind::NewtypeBody => RequestField::NewtypeBody(field),
            RequestFieldKind::Path => RequestField::Path(field),
            RequestFieldKind::Query => RequestField::Query(field),
        }
    }

    /// Gets the kind of the request field.
    fn kind(&self) -> RequestFieldKind {
        match self {
            RequestField::Body(..) => RequestFieldKind::Body,
            RequestField::Header(..) => RequestFieldKind::Header,
            RequestField::NewtypeBody(..) => RequestFieldKind::NewtypeBody,
            RequestField::Path(..) => RequestFieldKind::Path,
            RequestField::Query(..) => RequestFieldKind::Query,
        }
    }

    /// Whether or not this request field is a body kind.
    fn is_body(&self) -> bool {
        self.kind() == RequestFieldKind::Body
    }

    /// Whether or not this request field is a header kind.
    fn is_header(&self) -> bool {
        self.kind() == RequestFieldKind::Header
    }

    /// Whether or not this request field is a path kind.
    fn is_path(&self) -> bool {
        self.kind() == RequestFieldKind::Path
    }

    /// Whether or not this request field is a query string kind.
    fn is_query(&self) -> bool {
        self.kind() == RequestFieldKind::Query
    }

    /// Return the contained field if this request field is a body kind.
    fn as_body_field(&self) -> Option<&Field> {
        self.field_of_kind(RequestFieldKind::Body)
    }

    /// Return the contained field if this request field is a body kind.
    fn as_newtype_body_field(&self) -> Option<&Field> {
        self.field_of_kind(RequestFieldKind::NewtypeBody)
    }

    /// Return the contained field if this request field is a path kind.
    fn as_path_field(&self) -> Option<&Field> {
        self.field_of_kind(RequestFieldKind::Path)
    }

    /// Return the contained field if this request field is a query kind.
    fn as_query_field(&self) -> Option<&Field> {
        self.field_of_kind(RequestFieldKind::Query)
    }

    /// Gets the inner `Field` value.
    fn field(&self) -> &Field {
        match self {
            RequestField::Body(field)
            | RequestField::Header(field, _)
            | RequestField::NewtypeBody(field)
            | RequestField::Path(field)
            | RequestField::Query(field) => field,
        }
    }

    /// Gets the inner `Field` value if it's of the provided kind.
    fn field_of_kind(&self, kind: RequestFieldKind) -> Option<&Field> {
        if self.kind() == kind {
            Some(self.field())
        } else {
            None
        }
    }
}

/// The types of fields that a request can have, without their values.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RequestFieldKind {
    /// See the similarly named variant of `RequestField`.
    Body,
    /// See the similarly named variant of `RequestField`.
    Header,
    /// See the similarly named variant of `RequestField`.
    NewtypeBody,
    /// See the similarly named variant of `RequestField`.
    Path,
    /// See the similarly named variant of `RequestField`.
    Query,
}
