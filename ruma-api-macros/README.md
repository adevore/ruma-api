# ruma-api-macros

[![Build Status](https://travis-ci.org/ruma/ruma-api.svg?branch=master)](https://travis-ci.org/ruma/ruma-api)

**ruma-api-macros** provides a procedural macro for easily generating [ruma-api](https://github.com/ruma/ruma-api)-compatible API endpoints.
You define the endpoint's metadata, request fields, and response fields, and the macro generates all the necessary types and implements all the necessary traits.

## Usage

Here is an example that shows most of the macro's functionality.

``` rust
pub mod some_endpoint {
    use ruma_api_macros::ruma_api;

    ruma_api! {
        metadata {
            description: "Does something.",
            method: GET, // An `http::Method` constant. No imports required.
            name: "some_endpoint",
            path: "/_matrix/some/endpoint/:baz", // Variable path components start with a colon.
            rate_limited: false,
            requires_authentication: false,
        }

        request {
            // With no attribute on the field, it will be put into the body of the request.
            pub foo: String,

            // This value will be put into the "Content-Type" HTTP header.
            #[ruma_api(header = CONTENT_TYPE)]
            pub content_type: String

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
            #[ruma_api(header = CONTENT_TYPE)]
            pub content_type: String

            // With no attribute on the field, it will be extracted from the body of the response.
            pub value: String,
        }
    }
}
```

## Documentation

ruma-api-macros has [comprehensive documentation](https://docs.rs/ruma-api-macros) available on docs.rs.

## License

[MIT](http://opensource.org/licenses/MIT)
