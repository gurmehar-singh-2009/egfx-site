use std::fmt;

use snafu::prelude::*;
use wasm_bindgen::{JsCast, JsValue};

use crate::dom::ElmType;

#[derive(Debug)]
pub struct JsError {
    message: String,
}

impl JsError {
    fn from_js_value(value: JsValue) -> Self {
        let message = value
            .dyn_into::<js_sys::Error>()
            .map(|err| String::from(err.to_string()))
            .unwrap_or_else(|value| {
                value
                    .as_string()
                    .unwrap_or_else(|| "unknown JS error".to_owned())
            });

        Self { message }
    }
}

impl fmt::Display for JsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for JsError {}

#[derive(Debug, Snafu)]
#[snafu(visibility(pub(crate)))]
pub enum PageError {
    #[snafu(display("Failed to get the browser window!"))]
    GetWindow,

    #[snafu(display("Failed to get the document from the page!"))]
    GetDocument,

    #[snafu(display("Failed to get the document body from the page!"))]
    GetBody,

    #[snafu(display("Failed to create Element: {element_type}. {source}"))]
    ElementCreation {
        element_type: ElmType,
        #[snafu(source(from(JsValue, JsError::from_js_value)))]
        source: JsError,
    },

    #[snafu(display("Failed to append Element: {element_type}. {source}"))]
    AppendChild {
        element_type: ElmType,
        #[snafu(source(from(JsValue, JsError::from_js_value)))]
        source: JsError,
    },
}
