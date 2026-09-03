//! Boilerplate code for working with the DOM.
//! The system CAN be improved but probably is not worth it.

use snafu::ResultExt;
use web_sys::{Element, Node};

use crate::{
    BODY, DOC,
    errors::{AppendChildSnafu, ElementCreationSnafu, PageError},
};

#[derive(Debug, Default, Clone, Copy)]
/// The DOM element.
pub enum ElmType {
    #[default]
    /// The `div` element.
    Div,
    /// The `link` element.
    Link,
    /// The `style` element.
    Style,
    /// The `a` element.
    A,
    /// The `canvas` element.
    Canvas,
    /// The `pre` element.
    Pre,
    /// The `code` element.
    Code,
    /// The `span` element.
    Span,
}

impl ElmType {
    /// Returns the type as a &str with 'static.
    #[allow(clippy::trivially_copy_pass_by_ref)] // not the intention.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Div => "div",
            Self::Link => "link",
            Self::Style => "style",
            Self::A => "a",
            Self::Canvas => "canvas",
            Self::Code => "code",
            Self::Pre => "pre",
            Self::Span => "span",
        }
    }
}

impl std::fmt::Display for ElmType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Represents a DOM element in our system.
pub struct DomElement {
    /// The `web_sys::Element`.
    pub element: Element,
    /// The element type.
    pub element_type: ElmType,
}

impl DomElement {
    /// Appends the provided element to the list of children of this element.
    pub fn append_child(&self, child: &Self) -> Result<Node, PageError> {
        self.element
            .append_child(&child.element)
            .context(AppendChildSnafu {
                element_type: child.element_type,
            })
    }

    /// Appends this element directly to the <body> element.
    pub fn append_to_body(&self) -> Result<Node, PageError> {
        BODY.with(|body| {
            body.append_child(&self.element).context(AppendChildSnafu {
                element_type: self.element_type,
            })
        })
    }
}

#[derive(Debug, Default)]
/// Describes the properties we can set during the building phase.
pub struct DomElmBuilder<'a> {
    /// The element's type.
    element_type: ElmType,
    /// The element's id.
    id: Option<&'a str>,
    /// The element's class.
    class: Option<&'a str>,
    /// The `innerText` attribute for an element.
    text: Option<&'a str>,
    /// The `src` attribute for an element.
    src: Option<&'a str>,
    /// The `href` attribute for an element.
    href: Option<&'a str>,
    /// The `rel` attribute for an element.
    rel: Option<&'a str>,
    /// The `nonce` attribute for an element.
    nonce: Option<&'a str>,
    /// The `innerHTML` value for an element.
    inner_html: Option<&'a str>,
    /// The `style` value for an element.
    style: Option<String>,
    /// The children of this element.
    #[allow(clippy::use_self)] // `Self` cannot work here as it is bounded by `'a`.
    children: Vec<DomElmBuilder<'a>>,
}

impl<'a> DomElmBuilder<'a> {
    /// Begin building a new DOM element. Accepts the element type.
    pub fn new(element_type: ElmType) -> Self {
        Self {
            element_type,
            ..Default::default()
        }
    }

    /// Sets the element's `id` attribute.
    pub const fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }

    /// Sets the element's `class` attribute.
    pub const fn class(mut self, class: &'a str) -> Self {
        self.class = Some(class);
        self
    }

    /// Sets the element's text value.
    pub const fn text(mut self, text: &'a str) -> Self {
        self.text = Some(text);
        self
    }

    /// Sets the element's `src` attribute.
    #[allow(unused)] // may be useful in the future
    pub const fn src(mut self, src: &'a str) -> Self {
        self.src = Some(src);
        self
    }

    /// Sets the element's `href` attribute.
    pub const fn href(mut self, href: &'a str) -> Self {
        self.href = Some(href);
        self
    }

    /// Sets the element's `rel` attribute.
    pub const fn rel(mut self, rel: &'a str) -> Self {
        self.rel = Some(rel);
        self
    }

    #[allow(unused)] // may be useful in the future
    /// Sets the element's `nonce` attribute.
    pub const fn nonce(mut self, nonce: &'a str) -> Self {
        self.nonce = Some(nonce);
        self
    }

    /// Sets the element's `innerHTML` value.
    pub const fn html(mut self, html: &'a str) -> Self {
        self.inner_html = Some(html);
        self
    }

    /// Sets the element's inline style.
    pub fn style(mut self, style: impl Into<String>) -> Self {
        self.style = Some(style.into());
        self
    }

    /// Adds the child to the children list of this element.
    #[allow(clippy::missing_const_for_fn)] // This fn cannot be `const`.
    #[allow(clippy::use_self)] // `Self` cannot be used here inplace of `DomElmBuilder` because it is bounded by `'a`.
    pub fn child(mut self, child: DomElmBuilder<'a>) -> Self {
        self.children.push(child);
        self
    }

    /// Adds the children to the children list of this element.
    #[allow(clippy::missing_const_for_fn)] // This fn cannot be `const`.
    #[allow(clippy::use_self)] // `Self` cannot be used here inplace of `DomElmBuilder` because it is bounded by `'a`.
    pub fn children(mut self, children: impl IntoIterator<Item = DomElmBuilder<'a>>) -> Self {
        self.children.extend(children);
        self
    }

    /// Finalizes the build process and creates the DOM element, returning it.
    pub fn build(self) -> Result<DomElement, PageError> {
        let Self {
            element_type,
            id,
            class,
            text,
            src,
            href,
            rel,
            nonce,
            inner_html,
            style,
            children,
        } = self;

        let element = DOC.with(|doc| {
            doc.create_element(element_type.as_str())
                .context(ElementCreationSnafu { element_type })
        })?;

        if let Some(id) = id {
            element.set_id(id);
        }
        if let Some(class) = class {
            element.set_class_name(class);
        }

        if let Some(text) = text {
            element.set_text_content(Some(text));
        }

        if let Some(src) = src {
            element.set_attribute("src", src).unwrap();
        }

        if let Some(href) = href {
            element.set_attribute("href", href).unwrap();
        }

        if let Some(rel) = rel {
            element.set_attribute("rel", rel).unwrap();
        }

        if let Some(nonce) = nonce {
            element.set_attribute("nonce", nonce).unwrap();
        }

        if let Some(html) = inner_html {
            element.set_inner_html(html);
        }

        if let Some(style) = style {
            element.set_attribute("style", &style).unwrap();
        }

        let parent = DomElement {
            element,
            element_type,
        };

        for child in children {
            let child = child.build()?;
            parent.append_child(&child)?;
        }

        Ok(parent)
    }

    /// Appends this element directly to the <body> element.
    pub fn append_to_body(self) -> Result<DomElement, PageError> {
        let dom_element = self.build()?;
        dom_element.append_to_body()?;
        Ok(dom_element)
    }
}
