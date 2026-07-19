//! OneManga theme template for Aidoku, ported from nyora-data-driven's
//! OnemangaEngine.kt. 24 kotatsu sources.
//!
//! These are single-series Elementor/WordPress sites: the whole "catalogue" is
//! one manga, served from the home page. There is no search, no pagination and
//! no chapter dates — so the template is deliberately tiny, and every one of
//! the 24 rows ships an empty config (only the domain differs).

#![no_std]
extern crate alloc;

pub use nyora_common::IMG_ATTRS;

use aidoku::{
    alloc::{string::String, string::ToString, vec::Vec},
    imports::{
        html::{Document, Element},
        net::{HttpMethod, Request},
    },
    prelude::*,
    Chapter, ContentRating, Manga, MangaPageResult, Page, PageContent, Result, Viewer,
};

pub struct OneMangaConfig {
    pub domain: &'static str,
    pub nsfw: bool,
    pub locale: &'static str,
}

impl OneMangaConfig {
    pub fn base_url(&self) -> String {
        format!("https://{}", self.domain)
    }
}

const SEL_COVER: &str = "div.elementor-widget-container img";
const SEL_TITLE: &str = "ul.elementor-nav-menu li a";
const SEL_TEXT_ITEM: &str = "div.elementor-widget-text-editor ul li";
const SEL_CHAPTER_LINKS: &str = "ul li a";
const AUTHOR_LABEL: &str = "Auteur(s)";
const ALT_LABEL: &str = "Nom(s) Alternatif(s)";

pub struct OneMangaSource {
    pub cfg: &'static OneMangaConfig,
}

impl OneMangaSource {
    pub const fn new(cfg: &'static OneMangaConfig) -> Self {
        Self { cfg }
    }

    fn abs(&self, url: &str) -> String {
        if url.starts_with("http") {
            url.into()
        } else if let Some(r) = url.strip_prefix("//") {
            format!("https://{r}")
        } else if url.starts_with('/') {
            format!("{}{url}", self.cfg.base_url())
        } else {
            format!("{}/{url}", self.cfg.base_url())
        }
    }

    fn rel(&self, url: &str) -> String {
        let base = self.cfg.base_url();
        let s = url.strip_prefix(&base).unwrap_or(url);
        if s.starts_with('/') {
            s.into()
        } else if s.starts_with("http") {
            s.split_once("://")
                .and_then(|(_, r)| r.split_once('/'))
                .map(|(_, p)| format!("/{p}"))
                .unwrap_or_else(|| "/".into())
        } else {
            format!("/{s}")
        }
    }

    fn img(&self, el: &Element) -> Option<String> {
        nyora_common::img_src(el, |u| self.abs(u))
    }

    fn home(&self) -> Result<Document> {
        Ok(Request::new(self.cfg.base_url(), HttpMethod::Get)?.html()?)
    }

    // ---- listing -----------------------------------------------------------

    /// The site IS one manga, so the listing has exactly one entry and only a
    /// first page. A query is matched against that entry's title rather than
    /// sent anywhere — there is no search endpoint.
    pub fn search_filtered(
        &self,
        query: Option<String>,
        page: i32,
        _order: Option<&str>,
        _genre: Option<&str>,
    ) -> Result<MangaPageResult> {
        if page.max(1) > 1 {
            return Ok(MangaPageResult {
                entries: Vec::new(),
                has_next_page: false,
            });
        }
        let doc = self.home()?;
        let mut entries = self.parse_home(&doc);
        if let Some(q) = query.as_deref().filter(|q| !q.is_empty()) {
            let ql = q.to_lowercase();
            entries.retain(|m| m.title.to_lowercase().contains(&ql));
        }
        Ok(MangaPageResult {
            entries,
            has_next_page: false,
        })
    }

    pub fn search(&self, query: Option<String>, page: i32, order: Option<&str>) -> Result<MangaPageResult> {
        self.search_filtered(query, page, order, None)
    }

    pub fn listing(&self, _id: &str, page: i32) -> Result<MangaPageResult> {
        self.search_filtered(None, page, None, None)
    }

    fn parse_home(&self, doc: &Document) -> Vec<Manga> {
        let title = doc
            .select_first(SEL_TITLE)
            .and_then(|e| e.text())
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| self.cfg.domain.to_string());
        let cover = doc.select_first(SEL_COVER).and_then(|i| self.img(&i));
        alloc::vec![Manga {
            key: String::from("/"),
            title,
            cover,
            content_rating: if self.cfg.nsfw {
                ContentRating::NSFW
            } else {
                ContentRating::Safe
            },
            ..Default::default()
        }]
    }

    pub fn fetch_manga(&self, _key: &str) -> Result<Document> {
        self.home()
    }
    pub fn fetch_chapter(&self, key: &str) -> Result<Document> {
        Ok(Request::new(self.abs(key), HttpMethod::Get)?.html()?)
    }

    // ---- details -----------------------------------------------------------

    pub fn details(&self, manga: &mut Manga, doc: &Document) {
        if let Some(t) = doc.select_first(SEL_TITLE).and_then(|e| e.text()) {
            let t = t.trim();
            if !t.is_empty() {
                manga.title = t.into();
            }
        }
        manga.cover = doc
            .select_first(SEL_COVER)
            .and_then(|i| self.img(&i))
            .or_else(|| manga.cover.clone());

        // Author and alt-title are list items identified only by a label in
        // their own text; kotatsu selects them with :contains(), which isn't
        // real CSS.
        if let Some(a) = nyora_common::labelled_row(doc, SEL_TEXT_ITEM, &[AUTHOR_LABEL])
            .and_then(|e| e.text())
        {
            let v = strip_label(&a, AUTHOR_LABEL);
            if !v.is_empty() {
                manga.authors = Some(alloc::vec![v]);
            }
        }
        let alt = nyora_common::labelled_row(doc, SEL_TEXT_ITEM, &[ALT_LABEL])
            .and_then(|e| e.text())
            .map(|t| strip_label(&t, ALT_LABEL))
            .filter(|v| !v.is_empty());

        // The synopsis is the LAST text item, not the first — the earlier ones
        // are the labelled metadata rows above.
        let desc = nyora_common::select_nonempty(doc, SEL_TEXT_ITEM)
            .and_then(|l| l.last().and_then(|e| e.text()))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        manga.description = match (alt, desc) {
            (Some(a), Some(d)) => Some(format!("Alternative: {a}\n\n{d}")),
            (Some(a), None) => Some(format!("Alternative: {a}")),
            (None, d) => d,
        };
        manga.url = Some(self.cfg.base_url());
        manga.viewer = Viewer::Webtoon;
    }

    // ---- chapters ----------------------------------------------------------

    /// Chapters are links inside a single container, newest first. Numbering
    /// advances only on kept rows so dedup can't leave gaps.
    pub fn chapters(&self, doc: &Document) -> Vec<Chapter> {
        let root = doc.select_first("#All_chapters");
        let links = match &root {
            Some(r) => r.select(SEL_CHAPTER_LINKS).map(|l| l.collect::<Vec<_>>()),
            None => nyora_common::select_nonempty(doc, "#All_chapters a"),
        }
        .unwrap_or_default();

        let mut out = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        let mut index = 0f32;
        for a in links.iter().rev() {
            let Some(href) = a.attr("href") else { continue };
            let key = self.rel(&href);
            if key == "/" || seen.iter().any(|s| s == &key) {
                continue;
            }
            seen.push(key.clone());
            index += 1.0;
            out.push(Chapter {
                key: key.clone(),
                title: a
                    .text()
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty()),
                chapter_number: Some(index),
                // This family publishes no chapter dates at all.
                date_uploaded: None,
                url: Some(self.abs(&key)),
                ..Default::default()
            });
        }
        out.reverse();
        out
    }

    // ---- pages -------------------------------------------------------------

    pub fn pages(&self, doc: &Document) -> Result<Vec<Page>> {
        let mut out = Vec::new();
        if let Some(imgs) = nyora_common::select_nonempty(doc, SEL_COVER) {
            for img in imgs {
                if let Some(u) = self.img(&img) {
                    out.push(Page {
                        content: PageContent::Url(u, None),
                        ..Default::default()
                    });
                }
            }
        }
        Ok(out)
    }

    pub fn genres(&self) -> Result<Vec<(String, String)>> {
        Ok(Vec::new())
    }
}

fn strip_label(text: &str, label: &str) -> String {
    let t = text.trim();
    match t.find(label) {
        Some(i) => t[i + label.len()..]
            .trim_start_matches([':', ' ', '\u{a0}'])
            .trim()
            .to_string(),
        None => t.to_string(),
    }
}
