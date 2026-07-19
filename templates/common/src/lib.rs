//! Pieces shared by every Nyora engine template.
//!
//! Date handling and the image-src ladder are identical across the kotatsu
//! engines (MangaReaderEngine's `srcWith` is byte-identical to Madara's), so
//! they live here rather than being copied per template.

#![no_std]
extern crate alloc;

pub mod date;

/// Image src candidates, in priority order. `src` is LAST deliberately: these
/// themes lazy-load, so `src` is usually a placeholder and the real URL sits in
/// a data-* attribute. Getting this order wrong shows placeholders everywhere.
pub const IMG_ATTRS: &[&str] = &[
    "data-src",
    "data-cfsrc",
    "data-original",
    "data-cdn",
    "data-sizes",
    "data-lazy-src",
    "data-srcset",
    "original-src",
    "data-wpfc-original-src",
    "src",
];

use aidoku::{
    alloc::{string::String, vec::Vec},
    imports::html::{Document, Element},
};

/// `Document::select` returns `Some(empty_list)` — never `None` — when a
/// selector matches nothing, so `.or_else()` fallback chains built on it never
/// fire. Every multi-selector fallback in these templates must go through this
/// instead, or only the first selector in the chain is ever used.
pub fn select_nonempty(doc: &Document, sel: &str) -> Option<Vec<Element>> {
    let v: Vec<Element> = doc.select(sel)?.collect();
    if v.is_empty() { None } else { Some(v) }
}

/// Resolve an element's image URL against the ladder above. `abs` makes a URL
/// absolute for the calling source's domain.
pub fn img_src<F: Fn(&str) -> String>(el: &Element, abs: F) -> Option<String> {
    for attr in IMG_ATTRS {
        if let Some(v) = el.attr(attr) {
            let v = v.trim();
            if v.is_empty() || v.starts_with("data:") {
                continue;
            }
            // srcset: take the first candidate
            let first = v.split(',').next().unwrap_or(v);
            let first = first.split_whitespace().next().unwrap_or(first);
            if !first.is_empty() {
                return Some(abs(first));
            }
        }
    }
    None
}

/// Find the first element matching `sel` whose text contains any of `labels`.
///
/// kotatsu writes these as `dt:contains(Status)` / `li:contains(Auteur(s))`,
/// but `:contains()` is a Jsoup extension, not CSS — Aidoku's selector engine
/// rejects it outright. Every engine needs this, so it lives here rather than
/// being hand-rolled per template.
pub fn labelled_row(doc: &Document, sel: &str, labels: &[&str]) -> Option<Element> {
    let rows = select_nonempty(doc, sel)?;
    rows.into_iter().find(|r| {
        let t = r.text().unwrap_or_default().trim().to_lowercase();
        !t.is_empty() && labels.iter().any(|l| t.contains(&l.to_lowercase()))
    })
}

/// The leading text node of an element — kotatsu's `.html().substringBefore("<")`.
///
/// Not the same as `own_text()`, which concatenates ALL direct text nodes; these
/// themes rely on taking only the text before the first child tag.
pub fn leading_text(el: &Element) -> Option<String> {
    let h = el.html()?;
    let head = h.split('<').next().unwrap_or("").trim();
    if head.is_empty() { None } else { Some(head.into()) }
}

/// Pull a URL out of an inline `style="background-image: url(...)"`.
/// Several themes render covers as CSS backgrounds rather than <img>.
pub fn css_bg_url(style: &str) -> Option<String> {
    let i = style.find("url(")? + 4;
    let j = style[i..].find(')')?;
    let u = style[i..i + j].trim().trim_matches(|c| c == '"' || c == '\'');
    if u.is_empty() { None } else { Some(u.into()) }
}
