#![allow(clippy::multiple_crate_versions)] // The `syn` crate messes this up...

use snafu::OptionExt;
use syntect::{
    highlighting::ThemeSet,
    html::{ClassStyle, ClassedHTMLGenerator, css_for_theme_with_class_style},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};
use wasm_bindgen::prelude::*;
use web_sys::{Document, HtmlElement};

use crate::{
    dom::{DomElmBuilder, ElmType},
    errors::{GetBodySnafu, GetDocumentSnafu, GetWindowSnafu, PageError},
};

mod dom;
mod errors;

thread_local! {
    static DOC: Document = get_document().expect("Failed to get document!");
    static BODY: HtmlElement = get_body().expect("Failed to get body!");
}

/// The sample TypeScript code to showcase.
const EASYGFX_TS: &str = r#"import {
  Backends,
  Engine,
} from "https://cdn.jsdelivr.net/gh/gurmehar-singh-2009/easygfx@main/dist/index.js";

const gameCanvas = document.createElement("gameCanvas");
document.body.appendChild(gameCanvas);

const engine = new Engine(gameCanvas, {
  backend: Backends.WEBGPU,
  antialias: false,
  debug: false,
});

window.addEventListener("resize", () => {
  engine.resize(window.innerWidth, window.innerHeight);
});

engine.resize(window.innerWidth, window.innerHeight);

engine.start();

engine.onFrame = (renderer, timestamp) => {
  renderer.clear(0, 0, 0, 1);

  renderer.setColor(255, 0, 0, 1);
  renderer.drawSquare(40, 40, 50, 50);

  renderer.setColor(255, 255, 255, 1);
  renderer.drawText(50, 100, "Working Demo!", 22);
};
"#;

#[wasm_bindgen(start)]
fn main() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    web_sys::console::log_1(&"why peek into console?".into());

    start().map_err(|e| JsValue::from_str(&e.to_string()))
}

/// Renders the page.
fn start() -> Result<(), PageError> {
    inject_css();
    inject_syntax_theme_css();

    DomElmBuilder::new(ElmType::Div)
        .id("app-container")
        .children([
            // nav bar
            DomElmBuilder::new(ElmType::Div).id("navbar").children([
                // TODO: image placeholder
                DomElmBuilder::new(ElmType::Div).child(
                    DomElmBuilder::new(ElmType::Div).class("logo").text(">_EasyGFX")
                ),

                DomElmBuilder::new(ElmType::Div).id("navbar-entries").children([
                    DomElmBuilder::new(ElmType::Div).class("navbar-entry").text("Demos").href("https://gurmeharsingh2009.me/easygfx/"),
                    DomElmBuilder::new(ElmType::Div).class("navbar-entry").text("Documentation").href("https://gurmeharsingh2009.me/easygfx/"),
                    DomElmBuilder::new(ElmType::Div).class("navbar-entry").text("Installation").href("https://gurmeharsingh2009.me/easygfx/"),
                ]),
            ]),

            // hero element
            DomElmBuilder::new(ElmType::Div)
                .id("hero")
                .text("A backend-agnostic 2D/3D rendering engine for the web."),

            DomElmBuilder::new(ElmType::Div)
                .id("expanded-hero")
                .text("EasyGFX abstracts away the differences between Canvas2D, WebGL and WebGPU, giving you a simple, consistent API for building 2D and 3D experiences on the web. Choose the backend that fits your needs while keeping your rendering code, workflow, and mental model the same."),

            // DomElmBuilder::new(ElmType::Div)
            //     .id("extra-btns")
            //     .children([
            //         DomElmBuilder::new(ElmType::Div).class("extra-btn").text("Quick start"),
            //     ]),

            // code block example
            DomElmBuilder::new(ElmType::Div)
                .class("code-block")
                .html(highlight(EASYGFX_TS, "js").as_str()), // it doesn't support TS :(

            DomElmBuilder::new(ElmType::Div).class("footer").text("© Copyright 2026 EasyGFX"),
        ])
        .append_to_body()?;

    Ok(())
}

/// Returns a `Result` for a `web_sys::Document` handle.
fn get_document() -> Result<Document, PageError> {
    web_sys::window()
        .context(GetWindowSnafu)?
        .document()
        .context(GetDocumentSnafu)
}

/// Returns a `Result` for a `web_sys::HtmlElement` handle that corresponds to
/// the <body> element.
fn get_body() -> Result<HtmlElement, PageError> {
    DOC.with(Document::body).context(GetBodySnafu)
}

/// Injects the stylesheet into the DOM.
/// Technically should append to <head> but whatever.
fn inject_css() {
    _ = DomElmBuilder::new(ElmType::Link)
        .rel("stylesheet")
        .href("style.css")
        .append_to_body();
}

/// Injects the code-block css theme.
fn inject_syntax_theme_css() {
    let ts = ThemeSet::load_defaults();
    for name in ts.themes.keys() {
        web_sys::console::log_1(&name.clone().into());
    }
    let css = css_for_theme_with_class_style(
        &ts.themes["base16-ocean.dark"],
        ClassStyle::SpacedPrefixed { prefix: "syn-" },
    )
    .expect("failed to build syntect theme css");

    // bit of chrome for the <pre> wrapper
    let extra = r#"
pre.syn-code {
    color: #c0c5ce;
    padding: 1em;
    background: #1b1b1b;
    border-radius: 8px;
    overflow-x: auto;
    font-family: ui-monospace, "SF Mono", Menlo, Consolas, monospace;
    font-size: 14px;
    line-height: 1.5;
    tab-size: 4;
}
"#;

    _ = DomElmBuilder::new(ElmType::Style)
        .html(format!("{css}{extra}").as_str())
        .append_to_body();
}

/// Syntax Highlights a select block of code with specified language token.
fn highlight(code: &str, lang_token: &str) -> String {
    let ss = SyntaxSet::load_defaults_newlines();
    let syntax = ss
        .find_syntax_by_token(lang_token)
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let mut generator = ClassedHTMLGenerator::new_with_class_style(
        syntax,
        &ss,
        ClassStyle::SpacedPrefixed { prefix: "syn-" },
    );

    for line in LinesWithEndings::from(code) {
        generator
            .parse_html_for_line_which_includes_newline(line)
            .expect("Failed to generate HTML stuff.");
    }

    format!(
        "<pre class=\"syn-code\"><code>{}</code></pre>",
        generator.finalize()
    )
}
