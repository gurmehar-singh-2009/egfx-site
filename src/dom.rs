use snafu::ResultExt;
use web_sys::{Element, Node};

use crate::{
    BODY, DOC,
    errors::{AppendChildSnafu, ElementCreationSnafu, PageError},
};

#[derive(Debug, Default, Clone, Copy)]
pub enum ElmType {
    #[default]
    Div,
    Link,
    A,
    Style,
}

impl ElmType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Div => "div",
            Self::Link => "link",
            Self::A => "a",
            Self::Style => "style",
        }
    }
}

impl std::fmt::Display for ElmType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

pub struct DomElement {
    pub element: Element,
    pub element_type: ElmType,
}

impl DomElement {
    pub fn append_child(&self, child: &DomElement) -> Result<Node, PageError> {
        self.element
            .append_child(&child.element)
            .context(AppendChildSnafu {
                element_type: child.element_type,
            })
    }

    pub fn append_to_body(&self) -> Result<Node, PageError> {
        BODY.with(|body| {
            body.append_child(&self.element).context(AppendChildSnafu {
                element_type: self.element_type,
            })
        })
    }
}

#[derive(Debug, Default)]
pub struct DomElmBuilder<'a> {
    element_type: ElmType,
    id: Option<&'a str>,
    class: Option<&'a str>,
    text: Option<&'a str>,
    src: Option<&'a str>,
    href: Option<&'a str>,
    rel: Option<&'a str>,
    nonce: Option<&'a str>,
    inner_html: Option<&'a str>,
    children: Vec<DomElmBuilder<'a>>,
}

impl<'a> DomElmBuilder<'a> {
    pub fn new(element_type: ElmType) -> Self {
        Self {
            element_type,
            ..Default::default()
        }
    }

    pub fn id(mut self, id: &'a str) -> Self {
        self.id = Some(id);
        self
    }

    pub fn class(mut self, class: &'a str) -> Self {
        self.class = Some(class);
        self
    }

    pub fn text(mut self, text: &'a str) -> Self {
        self.text = Some(text);
        self
    }

    pub fn src(mut self, src: &'a str) -> Self {
        self.src = Some(src);
        self
    }

    pub fn href(mut self, href: &'a str) -> Self {
        self.href = Some(href);
        self
    }

    pub fn rel(mut self, rel: &'a str) -> Self {
        self.rel = Some(rel);
        self
    }

    pub fn nonce(mut self, nonce: &'a str) -> Self {
        self.nonce = Some(nonce);
        self
    }

    pub fn html(mut self, html: &'a str) -> Self {
        self.inner_html = Some(html);
        self
    }

    pub fn child(mut self, child: DomElmBuilder<'a>) -> Self {
        self.children.push(child);
        self
    }

    pub fn children(mut self, children: impl IntoIterator<Item = DomElmBuilder<'a>>) -> Self {
        self.children.extend(children);
        self
    }

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

        if let Some(nonce) = &self.nonce {
            element.set_attribute("nonce", nonce).unwrap();
        }

        if let Some(html) = inner_html {
            element.set_inner_html(html);
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

    pub fn append_to_body(self) -> Result<DomElement, PageError> {
        let dom_element = self.build()?;
        dom_element.append_to_body()?;
        Ok(dom_element)
    }
}
