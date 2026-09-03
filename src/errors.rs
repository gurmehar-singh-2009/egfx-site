//! A bunch of errors that could occur.

use std::fmt;

use snafu::prelude::*;
use wasm_bindgen::{JsCast, JsValue};

use crate::dom::ElmType;

#[derive(Debug)]
/// A JavaScript error. Used to convert it into something Rust can handle.
pub struct JsError {
    /// The error message.
    message: String,
}

impl JsError {
    /// Converts the JS Error into the Rust struct.
    fn from_js_value(value: JsValue) -> Self {
        let message = value.dyn_into::<js_sys::Error>().map_or_else(
            |value| {
                value
                    .as_string()
                    .unwrap_or_else(|| "unknown JS error".to_owned())
            },
            |err| String::from(err.to_string()),
        );

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
/// Contains all possible errors that could occur (except for where I got lazy
/// and just did `.expect(...)`).
pub enum PageError {
    #[snafu(display("Failed to get the browser window!"))]
    /// Unable to get a handle for the browser window.
    GetWindow,

    #[snafu(display("Failed to get the document from the page!"))]
    /// Unable to get a handle for the document.
    GetDocument,

    #[snafu(display("Failed to get the document body from the page!"))]
    /// Unable to get a handle for the document body.
    GetBody,

    #[snafu(display("Failed to create Element: {element_type}. {source}"))]
    /// Error while creating a DOM element.
    ElementCreation {
        /// The element type.
        element_type: ElmType,

        #[snafu(source(from(JsValue, JsError::from_js_value)))]
        /// The JS error.
        source: JsError,
    },

    #[snafu(display("Failed to append Element: {element_type}. {source}"))]
    /// Error while appending a DOM element.
    AppendChild {
        /// The element type.
        element_type: ElmType,

        #[snafu(source(from(JsValue, JsError::from_js_value)))]
        /// The JS error.
        source: JsError,
    },

    #[snafu(display("Failed to acquire the Canvas2d context from a canvas: {source}"))]
    Canvas2dContextAcquisition {
        #[snafu(source(from(JsValue, JsError::from_js_value)))]
        /// The JS error.
        source: JsError,
    },

    #[snafu(display("Failed to cast generic element into canvas element: {source}"))]
    GenericElementToCanvasConversionFailure {
        #[snafu(source(from(JsValue, JsError::from_js_value)))]
        /// The JS error.
        source: JsError,
    },

    #[snafu(display("Failed to draw for some reason: {source}"))]
    GenericDrawFailure {
        #[snafu(source(from(JsValue, JsError::from_js_value)))]
        /// The JS error.
        source: JsError,
    },

    #[snafu(display("Failed to serialize page to xhtml: {source}"))]
    XHTMLSerializationFailure {
        #[snafu(source(from(JsValue, JsError::from_js_value)))]
        /// The JS error.
        source: JsError,
    },
}
