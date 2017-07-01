#![feature(associated_consts, proc_macro, try_from)]

extern crate futures;
extern crate hyper;
extern crate ruma_api;
extern crate ruma_api_macros;
extern crate serde;
#[macro_use] extern crate serde_derive;
extern crate serde_json;
extern crate serde_urlencoded;
extern crate url;

pub mod some_endpoint {
    use hyper::header::ContentType;
    use ruma_api_macros::ruma_api;

    ruma_api! {
        metadata {
            description: "Does something.",
            method: Method::Get, // A `hyper::Method` value. No need to import the name.
            name: "some_endpoint",
            path: "/_matrix/some/endpoint/:baz",
            rate_limited: false,
            requires_authentication: false,
        }

        request {
            // With no attribute on the field, it will be put into the body of the request.
            pub foo: String,

            // This value will be put into the "Content-Type" HTTP header.
            #[ruma_api(header)]
            pub content_type: ContentType,

            // This value will be put into the query string of the request's URL.
            #[ruma_api(query)]
            pub bar: String,

            // This value will be inserted into the request's URL in place of the
            // ":baz" path component.
            #[ruma_api(path)]
            pub baz: String,
        }

        response {
            // This value will be extracted from the "Content-Type" HTTP header.
            #[ruma_api(header)]
            pub content_type: ContentType,

            // With no attribute on the field, it will be extracted from the body of the response.
            pub value: String,
        }
    }
}
