#![allow(clippy::multiple_crate_versions)] // The `syn` crate messes this up...
#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // canvas sizes etc.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use js_sys::Uint8Array;
use snafu::{OptionExt, ResultExt};
use syntect::{
    highlighting::ThemeSet,
    html::{ClassStyle, ClassedHTMLGenerator, css_for_theme_with_class_style},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};
use wasm_bindgen::{JsCast, closure::Closure, prelude::*};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::{
    Blob, BlobPropertyBag, CanvasRenderingContext2d, Document, HtmlCanvasElement, HtmlElement,
    HtmlImageElement, KeyboardEvent, MouseEvent, Node, OffscreenCanvas,
    OffscreenCanvasRenderingContext2d, Url, WheelEvent, XmlSerializer, window,
};

mod dom;
mod errors;

use crate::{
    dom::{DomElement, DomElmBuilder, ElmType},
    errors::{
        AppendChildSnafu, Canvas2dContextAcquisitionSnafu, GenericDrawFailureSnafu,
        GenericElementToCanvasConversionFailureSnafu, GetBodySnafu, GetDocumentSnafu,
        GetWindowSnafu, PageError, XHTMLSerializationFailureSnafu,
    },
};

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

const PAGE_CSS: &str = include_str!("../style.css");

/// Extra styles for the `<pre>` wrapper.
const PRE_CSS: &str = r#"
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

/// I was advised to do this.
const MAX_DPR: f64 = 2.0;

/// Fix for Safari.
const MAX_LAYER_AREA: f64 = 32.0 * 1024.0 * 1024.0;

#[wasm_bindgen(start)]
fn main() -> Result<(), JsValue> {
    console_error_panic_hook::set_once();
    web_sys::console::log_1(&"why peek into console?".into());

    spawn_local(async {
        if let Err(e) = start().await {
            web_sys::console::error_1(&format!("{e}").into());
        }
    });

    Ok(())
}

/// Renders the page. To a canvas. Like it's 1995.
async fn start() -> Result<(), PageError> {
    // The ONLY element that ever lives in the DOM (besides the other allowed tags
    // :P).
    let canvas = DomElmBuilder::new(ElmType::Canvas)
        .id("main-canvas")
        .build()?;

    canvas.append_to_body()?;

    let page = Rc::new(Page::new(canvas)?);

    page.install_listeners();
    page.rasterize().await?;
    page.start_loop();

    Ok(())
}

/// A clickable region of the rasterized page.
struct Link {
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    href: String,
}

/// Everything that isn't the main canvas.
struct Page {
    /// The only element in the DOM.
    main: HtmlCanvasElement,
    /// Rendering context for the main canvas element.
    main_ctx: CanvasRenderingContext2d,
    /// Holds the rasterized page. Never touches the DOM at all.
    layer: OffscreenCanvas,
    /// Rendering context for the Offscreen canvas.
    layer_ctx: OffscreenCanvasRenderingContext2d,
    /// Device px per CSS px the layer is rendered at.
    scale: Cell<f64>,
    /// Full page height, CSS px.
    content_h: Cell<f64>,
    /// Scroll offset, CSS px.
    scroll_y: Cell<f64>,
    /// Clickable rects in page coordinates (CSS px).
    links: RefCell<Vec<Link>>,
    /// Last mouse position, viewport CSS px (to re-hover after scrolling).
    mouse: Cell<(f64, f64)>,
    hovered: Cell<Option<usize>>,
    /// Needs a full re-rasterize (resize / content change).
    dirty: Cell<bool>,
    rasterizing: Cell<bool>,
    last_raster_ms: Cell<f64>,
}

impl Page {
    fn new(main: DomElement) -> Result<Self, PageError> {
        let main: HtmlCanvasElement = main
            .element
            .dyn_into()
            .expect("main element should be a <canvas>!!");

        let main_ctx: CanvasRenderingContext2d = main
            .get_context("2d")
            .context(Canvas2dContextAcquisitionSnafu)?
            .expect("context '2d' returned null")
            .dyn_into()
            .expect("error too lazy to write msg");

        let layer = OffscreenCanvas::new(1, 1);
        let layer_ctx: OffscreenCanvasRenderingContext2d = layer
            .as_ref()
            .unwrap() // Safe unwrap
            .get_context("2d")
            .context(Canvas2dContextAcquisitionSnafu)?
            .unwrap()
            .unchecked_into();

        // The canvas IS the page now.
        let s = main.style();
        for (prop, value) in [
            ("display", "block"),
            ("position", "fixed"),
            ("top", "0"),
            ("left", "0"),
            ("width", "100vw"),
            ("height", "100vh"),
            ("cursor", "default"),
        ] {
            s.set_property(prop, value).expect("set canvas css");
        }

        // No browser-owned scrolling (we own it), and a dark backdrop so
        // there's no white flash before the first raster lands.
        let bs = BODY.with(|b| b.style());
        _ = bs.set_property("margin", "0");
        _ = bs.set_property("overflow", "hidden");
        _ = bs.set_property("background", "#000");

        Ok(Self {
            main,
            main_ctx,
            layer: layer.unwrap(),
            layer_ctx,
            scale: Cell::new(MAX_DPR),
            content_h: Cell::new(0.0),
            scroll_y: Cell::new(0.0),
            links: RefCell::new(Vec::new()),
            mouse: Cell::new((0.0, 0.0)),
            hovered: Cell::new(None),
            dirty: Cell::new(false),
            rasterizing: Cell::new(false),
            last_raster_ms: Cell::new(0.0),
        })
    }

    /// Rebuilds + re-rasterizes the page into the offscreen layer, then blits.
    async fn rasterize(&self) -> Result<(), PageError> {
        let (vw, vh, dpr) = viewport();

        let css = page_css();

        let root = build_page_tree()?;

        let (links, measured_h) = measure_page(&root, &css)?;
        *self.links.borrow_mut() = links;

        let content_h = measured_h.max(vh).min(20_000.0);
        self.content_h.set(content_h);

        let scale = ((MAX_LAYER_AREA / (vw * content_h)).sqrt())
            .min(dpr)
            .max(1.0);
        self.scale.set(scale);

        let body_html = serialize_xhtml(&root);
        let svg = wrap_in_svg(&body_html, &css, vw, content_h, vw, vh);

        let (img, url) = load_svg_image(&svg).await;

        self.main.set_width((vw * dpr).round() as u32);
        self.main.set_height((vh * dpr).round() as u32);
        self.layer.set_width((vw * scale).round() as u32);
        self.layer.set_height((content_h * scale).round() as u32);

        self.layer_ctx
            .draw_image_with_html_image_element_and_dw_and_dh(
                &img,
                0.0,
                0.0,
                self.layer.width() as f64,
                self.layer.height() as f64,
            )
            .context(GenericDrawFailureSnafu)?;
        Url::revoke_object_url(&url);

        let max = (self.content_h.get() - vh).max(0.0);
        self.scroll_y.set(self.scroll_y.get().min(max));

        self.last_raster_ms.set(now_ms());
        self.blit();

        Ok(())
    }

    fn blit(&self) {
        let (_, vh, dpr) = viewport();

        self.main_ctx.clear_rect(
            0.0,
            0.0,
            self.main.width() as f64,
            self.main.height() as f64,
        );

        let scale = self.scale.get();
        let layer_w = self.layer.width() as f64;
        let layer_h = self.layer.height() as f64;

        let sy = (self.scroll_y.get() * scale).min((layer_h - vh * scale).max(0.0));
        let sh = (vh * scale).min(layer_h - sy);

        if sh > 0.0 {
            let dh = sh * (dpr / scale);
            // sonion WHAT is this method name
            self.main_ctx
                .draw_image_with_offscreen_canvas_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                    &self.layer,
                    0.0,
                    sy,
                    layer_w,
                    sh,
                    0.0,
                    0.0,
                    self.main.width() as f64,
                    dh,
                )
                .context(GenericDrawFailureSnafu)
                .unwrap();
        }

        self.draw_scrollbar();
        self.draw_hover_underline();
    }

    fn draw_scrollbar(&self) {
        let (vw, vh, dpr) = viewport();
        let content_h = self.content_h.get();
        if content_h <= vh {
            return; // everything fits
        }

        let max_scroll = content_h - vh;
        let track_h = vh - 16.0;
        let thumb_h = (track_h * vh / content_h).clamp(40.0, track_h);
        let thumb_y = 8.0 + (self.scroll_y.get() / max_scroll) * (track_h - thumb_h);

        let ctx = &self.main_ctx;
        ctx.set_fill_style_str("rgba(255, 255, 255, 0.18)");
        ctx.fill_rect((vw - 10.0) * dpr, thumb_y * dpr, 6.0 * dpr, thumb_h * dpr);
    }

    fn draw_hover_underline(&self) {
        let Some(idx) = self.hovered.get() else {
            return;
        };
        let (_, _, dpr) = viewport();
        let links = self.links.borrow();
        let Some(link) = links.get(idx) else {
            return;
        };
        let scroll = self.scroll_y.get();

        // this is the poor man's version jajaja :(.
        let ctx = &self.main_ctx;
        ctx.set_stroke_style_str("rgba(255, 255, 255, 1.0)");
        ctx.set_line_width(2.0 * dpr);
        ctx.begin_path();
        ctx.move_to(link.x * dpr, (link.y - scroll + link.h) * dpr);
        ctx.line_to((link.x + link.w) * dpr, (link.y - scroll + link.h) * dpr);
        ctx.stroke();
    }

    fn scroll_by(&self, dy: f64) {
        let (_, vh, _) = viewport();
        let max = (self.content_h.get() - vh).max(0.0);
        let next = (self.scroll_y.get() + dy).clamp(0.0, max);

        if next != self.scroll_y.get() {
            self.scroll_y.set(next);
            self.update_hover();
            self.blit();
        }
    }

    fn update_hover(&self) {
        let (mx, my) = self.mouse.get();
        let scroll = self.scroll_y.get();

        let hit = self.links.borrow().iter().position(|l| {
            mx >= l.x && mx <= l.x + l.w && my + scroll >= l.y && my + scroll <= l.y + l.h
        });

        if hit != self.hovered.get() {
            self.hovered.set(hit);
            _ = self
                .main
                .style()
                .set_property("cursor", if hit.is_some() { "pointer" } else { "default" });
        }
    }

    fn click(&self) {
        let Some(idx) = self.hovered.get() else {
            return;
        };

        let href = self.links.borrow()[idx].href.clone();

        if href.is_empty() {
            return;
        }

        _ = window()
            .expect("no window")
            .open_with_url_and_target(&href, "_blank");
    }

    /// Wires up all event listeners.
    fn install_listeners(self: &Rc<Self>) {
        let win = window().expect("no window!?");

        // resize -> re-rasterize
        // so ugly
        {
            let page = Rc::clone(self);
            let on_resize = Closure::<dyn FnMut()>::new(move || page.dirty.set(true));
            win.add_event_listener_with_callback("resize", on_resize.as_ref().unchecked_ref())
                .expect("add resize listener");
            on_resize.forget();
        }

        // onclick events.
        {
            let page = Rc::clone(self);
            let on_click = Closure::<dyn FnMut(MouseEvent)>::new(move |e: MouseEvent| {
                page.mouse.set((e.client_x() as f64, e.client_y() as f64));
                page.update_hover();
                page.click();
            });
            self.main
                .add_event_listener_with_callback("click", on_click.as_ref().unchecked_ref())
                .expect("add click listener");
            on_click.forget();
        }

        // mousemove -> hover state and cursor.
        {
            let page = Rc::clone(self);
            let on_move = Closure::<dyn FnMut(MouseEvent)>::new(move |e: MouseEvent| {
                page.mouse.set((e.client_x() as f64, e.client_y() as f64));
                page.update_hover();
            });
            self.main
                .add_event_listener_with_callback("mousemove", on_move.as_ref().unchecked_ref())
                .expect("add mousemove listener");
            on_move.forget();
        }

        // wheel -> scroll
        {
            let page = Rc::clone(self);
            let on_wheel = Closure::<dyn FnMut(WheelEvent)>::new(move |e: WheelEvent| {
                e.prevent_default();
                let dy = match e.delta_mode() {
                    1 => e.delta_y() * 16.0,         // lines
                    2 => e.delta_y() * viewport().1, // pages
                    _ => e.delta_y(),                // pixels
                };
                page.scroll_by(dy);
            });
            self.main
                .add_event_listener_with_callback("wheel", on_wheel.as_ref().unchecked_ref())
                .expect("add wheel listener");
            on_wheel.forget();
        }

        // keyboard scroll
        {
            let page = Rc::clone(self);
            let on_key = Closure::<dyn FnMut(KeyboardEvent)>::new(move |e: KeyboardEvent| {
                let (_, vh, _) = viewport();
                let dy = match e.key().as_str() {
                    "ArrowDown" => Some(80.0),
                    "ArrowUp" => Some(-80.0),
                    "PageDown" => Some(vh * 0.85),
                    "PageUp" => Some(-vh * 0.85),
                    "Home" => {
                        e.prevent_default();
                        page.scroll_by(-f64::INFINITY);
                        None
                    }
                    "End" => {
                        e.prevent_default();
                        page.scroll_by(f64::INFINITY);
                        None
                    }
                    _ => None,
                };
                if let Some(dy) = dy {
                    e.prevent_default();
                    page.scroll_by(dy);
                }
            });
            win.add_event_listener_with_callback("keydown", on_key.as_ref().unchecked_ref())
                .expect("add keydown listener");
            on_key.forget();
        }
    }

    fn start_loop(self: &Rc<Self>) {
        let page = Rc::clone(self);
        let f: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> = Rc::new(RefCell::new(None));
        let g = Rc::clone(&f);

        // safe
        *g.borrow_mut() = Some(Closure::new(move |_ts| {
            // probably only need to do it every 150ms
            if page.dirty.get()
                && !page.rasterizing.get()
                && now_ms() - page.last_raster_ms.get() >= 150.0
            {
                page.dirty.set(false);
                page.rasterizing.set(true);
                let p = Rc::clone(&page);
                spawn_local(async move {
                    if let Err(e) = p.rasterize().await {
                        web_sys::console::error_1(&format!("{e}").into());
                    }
                    p.rasterizing.set(false);
                });
            }

            page.blit();

            request_frame(&f);
        }));

        request_frame(&g);
    }
}

fn measure_page(root: &DomElement, css: &str) -> Result<(Vec<Link>, f64), PageError> {
    root.element
        .set_attribute("style", "visibility:hidden")
        .expect("set measure style");

    let style = DomElmBuilder::new(ElmType::Style).html(css).build()?;

    BODY.with(|body| -> Result<Node, PageError> {
        body.append_child(&style.element)
            .context(AppendChildSnafu {
                element_type: ElmType::Style,
            })?;
        body.append_child(&root.element).context(AppendChildSnafu {
            element_type: root.element_type,
        })
    })?;

    // reading these forces layout.
    let body_h = BODY.with(|b| b.scroll_height()) as f64;
    let root_h = root.element.get_bounding_client_rect().height();
    let content_h = body_h.max(root_h);

    let mut links = Vec::new();
    let anchors = root
        .element
        .query_selector_all("a")
        .expect("query <a> elements");

    for i in 0..anchors.length() {
        let anchor: HtmlElement = anchors
            .get(i)
            .expect("anchor node")
            .dyn_into()
            .expect("anchor is an element");
        let rect = anchor.get_bounding_client_rect();
        if rect.width() <= 0.0 || rect.height() <= 0.0 {
            continue;
        }
        links.push(Link {
            x: rect.left(),
            y: rect.top(),
            w: rect.width(),
            h: rect.height(),
            href: anchor.get_attribute("href").unwrap_or_default(),
        });
    }

    BODY.with(|body| {
        _ = body.remove_child(&root.element);
        _ = body.remove_child(&style.element);
    });
    root.element
        .remove_attribute("style")
        .expect("remove measure style");

    Ok((links, content_h))
}

fn serialize_xhtml(root: &DomElement) -> String {
    XmlSerializer::new()
        .unwrap()
        .serialize_to_string(&root.element)
        // .context(XHTMLSerializationFailureSnafu)
        .expect("serialize page to XHTML")
}

fn wrap_in_svg(
    body_html: &str,
    css: &str,
    w: f64,
    h: f64,
    viewport_w: f64,
    viewport_h: f64,
) -> String {
    let css = rewrite_viewport_units(css, viewport_w, viewport_h);

    let css = css.replace('&', "&amp;").replace('<', "&lt;");

    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w}" height="{h}">
  <foreignObject width="{w}" height="{h}">
    <html xmlns="http://www.w3.org/1999/xhtml">
      <head><style>{css}</style></head>
      <body style="margin:0;width:{w}px;">{body_html}</body>
    </html>
  </foreignObject>
</svg>"#
    )
}

fn rewrite_viewport_units(css: &str, vw: f64, vh: f64) -> String {
    let css = css
        .replace("dvh", "vh")
        .replace("svh", "vh")
        .replace("lvh", "vh")
        .replace("dvw", "vw")
        .replace("svw", "vw")
        .replace("lvw", "vw");

    let b = css.as_bytes();
    let mut out = String::with_capacity(css.len() + 32);
    let mut last = 0;
    let mut i = 0;

    while i + 1 < b.len() {
        if b[i] == b'v' && (b[i + 1] == b'h' || b[i + 1] == b'w') {
            let mut start = i;
            while start > 0 && (b[start - 1].is_ascii_digit() || b[start - 1] == b'.') {
                start -= 1;
            }
            if start > 0
                && b[start - 1] == b'-'
                && (start < 2
                    || !(b[start - 2].is_ascii_alphanumeric()
                        || b[start - 2] == b'-'
                        || b[start - 2] == b'_'))
            {
                start -= 1;
            }

            let has_digit = b[start..i].iter().any(u8::is_ascii_digit);
            let boundary_ok = start == 0
                || !(b[start - 1].is_ascii_alphanumeric()
                    || b[start - 1] == b'_'
                    || b[start - 1] == b'-');

            if has_digit && boundary_ok {
                if let Ok(value) = css[start..i].parse::<f64>() {
                    let base = if b[i + 1] == b'h' { vh } else { vw };
                    out.push_str(&css[last..start]);
                    out.push_str(&format!("{:.2}px", value / 100.0 * base));
                    last = i + 2;
                    i += 2;
                    continue;
                }
            }
        }
        i += 1;
    }

    out.push_str(&css[last..]);
    out
}

async fn load_svg_image(svg: &str) -> (HtmlImageElement, String) {
    let bytes = Uint8Array::new_with_length(svg.len() as u32);
    bytes.copy_from(svg.as_bytes());

    let blob = Blob::new_with_u8_array_sequence_and_options(
        &js_sys::Array::of1(&bytes.into()),
        &BlobPropertyBag::new().type_("image/svg+xml"),
    )
    .expect("failed to build SVG blob");

    let url = Url::create_object_url_with_blob(&blob).expect("failed to create blob url");

    let img: HtmlImageElement = DOC
        .with(|doc| doc.create_element("img"))
        .expect("failed to create <img>")
        .dyn_into()
        .expect("created element is not an image");

    let loaded = js_sys::Promise::new(&mut |resolve, reject| {
        img.set_onload(Some(resolve.unchecked_ref()));
        img.set_onerror(Some(reject.unchecked_ref()));
        img.set_src(&url);
    });

    if let Err(e) = JsFuture::from(loaded).await {
        // Most likely the XHTML wasn't well-formed (a stray `&` or `<`).
        web_sys::console::error_1(
            &format!(
                "SVG rasterization failed: {e:?}\n--- svg head ---\n{}",
                &svg[..svg.len().min(1000)]
            )
            .into(),
        );
        panic!("failed to rasterize the page SVG");
    }

    (img, url)
}

fn request_frame(f: &RefCell<Option<Closure<dyn FnMut(f64)>>>) {
    let cb = f.borrow();
    window()
        .expect("no window")
        .request_animation_frame(cb.as_ref().expect("frame closure").as_ref().unchecked_ref())
        .expect("requestAnimationFrame");
}

fn viewport() -> (f64, f64, f64) {
    let win = window().expect("no window");
    let w = win
        .inner_width()
        .expect("innerWidth")
        .as_f64()
        .unwrap_or(1280.0);
    let h = win
        .inner_height()
        .expect("innerHeight")
        .as_f64()
        .unwrap_or(720.0);
    let dpr = win.device_pixel_ratio().min(MAX_DPR);
    (w.max(1.0), h.max(1.0), dpr)
}

fn now_ms() -> f64 {
    js_sys::Date::now()
}

fn build_page_tree() -> Result<DomElement, PageError> {
    DomElmBuilder::new(ElmType::Div)
        .id("app-container")
        .children([
            // nav bar
            DomElmBuilder::new(ElmType::Div).id("navbar").children([
                // TODO: image placeholder
                DomElmBuilder::new(ElmType::Div).child(
                    DomElmBuilder::new(ElmType::Div).class("logo").text(">_EasyGFX"),
                ),
                DomElmBuilder::new(ElmType::Div).id("navbar-entries").children([
                    DomElmBuilder::new(ElmType::A).class("navbar-entry").text("Demos").href("https://gurmeharsingh2009.me/easygfx/"),
                    DomElmBuilder::new(ElmType::A).class("navbar-entry").text("Documentation").href("https://gurmeharsingh2009.me/easygfx/"),
                    DomElmBuilder::new(ElmType::A).class("navbar-entry").text("Installation").href("https://gurmeharsingh2009.me/easygfx/"),
                ]),
            ]),

            // hero element
            DomElmBuilder::new(ElmType::Div)
                .id("hero")
                .text("A backend-agnostic 2D/3D rendering engine for the web."),

            DomElmBuilder::new(ElmType::Div)
                .id("expanded-hero")
                .text("EasyGFX abstracts away the differences between Canvas2D, WebGL and WebGPU, giving you a simple, consistent API for building 2D and 3D experiences on the web. Choose the backend that fits your needs while keeping your rendering code, workflow, and mental model the same."),

            // code block example
            DomElmBuilder::new(ElmType::Div)
                .class("code-block")
                .html(highlight(EASYGFX_TS, "js").as_str()), // it doesn't support TS :(

            DomElmBuilder::new(ElmType::Div).class("footer").text("© Copyright 2026 EasyGFX"),
        ])
        .build()
}

fn page_css() -> String {
    format!("{PAGE_CSS}\n{}", syntax_theme_css())
}

fn syntax_theme_css() -> String {
    format!("{}{PRE_CSS}", theme_css())
}

fn syntax_set() -> &'static SyntaxSet {
    static SS: std::sync::OnceLock<SyntaxSet> = std::sync::OnceLock::new();
    SS.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_css() -> &'static str {
    static CSS: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    CSS.get_or_init(|| {
        let ts = ThemeSet::load_defaults();
        css_for_theme_with_class_style(
            &ts.themes["base16-ocean.dark"],
            ClassStyle::SpacedPrefixed { prefix: "syn-" },
        )
        .expect("failed to build syntect theme css")
    })
    .as_str()
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

/// Syntax Highlights a select block of code with specified language token.
fn highlight(code: &str, lang_token: &str) -> String {
    let ss = syntax_set();
    let syntax = ss
        .find_syntax_by_token(lang_token)
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let mut generator = ClassedHTMLGenerator::new_with_class_style(
        syntax,
        ss,
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
