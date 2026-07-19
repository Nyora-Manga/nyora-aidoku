//! MMRCMS theme template for Aidoku, ported from nyora-data-driven's
//! MmrcmsEngine.kt. 16 kotatsu sources.
//!
//! Two browse endpoints (a filter grid and a latest-release grid) that return
//! different markup, and a details block built from `<dt>`/`<dd>` label pairs.
//! The labels are localised per source, which is what the `label_*` config
//! fields carry.

#![no_std]
extern crate alloc;

pub mod config;

pub use config::MmrcmsConfig;
pub use nyora_common::{date, IMG_ATTRS};

use aidoku::{
    alloc::{string::String, string::ToString, vec::Vec},
    imports::{
        html::{Document, Element},
        net::{HttpMethod, Request},
    },
    prelude::*,
    Chapter, ContentRating, Manga, MangaPageResult, MangaStatus, Page, PageContent, Result, Viewer,
};

pub struct MmrcmsSource {
    pub cfg: &'static MmrcmsConfig,
}

impl MmrcmsSource {
    pub const fn new(cfg: &'static MmrcmsConfig) -> Self {
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

    fn get(&self, url: &str) -> Result<Document> {
        Ok(Request::new(url, HttpMethod::Get)?.html()?)
    }

    // ---- listing -----------------------------------------------------------

    /// Latest uses a dedicated endpoint with its own markup; everything else
    /// (popular, alphabetical, search, tag) goes through the filter grid. The
    /// two return different card shapes, so the parser is chosen by endpoint.
    pub fn search_filtered(
        &self,
        query: Option<String>,
        page: i32,
        order: Option<&str>,
        genre: Option<&str>,
    ) -> Result<MangaPageResult> {
        let p = page.max(1);
        let q = query.as_deref().unwrap_or("");
        let latest = order == Some("latest") && q.is_empty() && genre.is_none();

        let (url, is_latest) = if latest {
            (format!("{}/latest-release?page={}", self.cfg.base_url(), p), true)
        } else {
            let sort = match order {
                Some("alphabetical") => "name&asc=true",
                Some("alphabetical_desc") => "name&asc=false",
                Some("views_asc") => "views&asc=true",
                _ => "views&asc=false",
            };
            (
                format!(
                    "{}/{}/?page={}&author=&tag=&alpha={}&cat={}&sortBy={}",
                    self.cfg.base_url(),
                    self.cfg.list_url.trim_matches('/'),
                    p,
                    encode(q),
                    genre.unwrap_or(""),
                    sort
                ),
                false,
            )
        };
        let doc = self.get(&url)?;
        let entries = if is_latest {
            self.parse_latest(&doc)
        } else {
            self.parse_grid(&doc)
        };
        Ok(MangaPageResult {
            has_next_page: !entries.is_empty(),
            entries,
        })
    }

    pub fn search(&self, query: Option<String>, page: i32, order: Option<&str>) -> Result<MangaPageResult> {
        self.search_filtered(query, page, order, None)
    }

    pub fn listing(&self, id: &str, page: i32) -> Result<MangaPageResult> {
        let order = match id {
            "latest" => "latest",
            "alphabetical" => "alphabetical",
            _ => "popular",
        };
        self.search_filtered(None, page, Some(order), None)
    }

    fn parse_grid(&self, doc: &Document) -> Vec<Manga> {
        let mut out = Vec::new();
        let Some(rows) = nyora_common::select_nonempty(doc, "div.media") else {
            return out;
        };
        for row in rows {
            let Some(a) = row.select_first("a") else { continue };
            let Some(href) = a.attr("href") else { continue };
            let title = row
                .select_first("div.media-body h5")
                .and_then(|e| e.text())
                .unwrap_or_default()
                .trim()
                .to_string();
            if title.is_empty() {
                continue;
            }
            out.push(Manga {
                key: self.rel(&href),
                title,
                cover: row.select_first("img").and_then(|i| self.img(&i)),
                content_rating: self.rating(),
                ..Default::default()
            });
        }
        out
    }

    /// The latest-release grid ships no <img>: the cover is derived from the
    /// slug, which is why `img_updated` is configurable per source.
    fn parse_latest(&self, doc: &Document) -> Vec<Manga> {
        let mut out = Vec::new();
        let Some(rows) = nyora_common::select_nonempty(doc, "div.manga-item") else {
            return out;
        };
        for row in rows {
            let Some(a) = row.select_first("h3 a").or_else(|| row.select_first("a")) else {
                continue;
            };
            let Some(href) = a.attr("href") else { continue };
            let title = a.text().unwrap_or_default().trim().to_string();
            if title.is_empty() {
                continue;
            }
            let slug = href.trim_end_matches('/').rsplit('/').next().unwrap_or("");
            out.push(Manga {
                key: self.rel(&href),
                title,
                cover: row.select_first("img").and_then(|i| self.img(&i)).or_else(|| {
                    Some(format!(
                        "{}/uploads/manga/{}{}",
                        self.cfg.base_url(),
                        slug,
                        self.cfg.img_updated
                    ))
                }),
                content_rating: self.rating(),
                ..Default::default()
            });
        }
        out
    }

    fn rating(&self) -> ContentRating {
        if self.cfg.nsfw {
            ContentRating::NSFW
        } else {
            ContentRating::Safe
        }
    }

    pub fn fetch_manga(&self, key: &str) -> Result<Document> {
        self.get(&self.abs(key))
    }
    pub fn fetch_chapter(&self, key: &str) -> Result<Document> {
        self.get(&self.abs(key))
    }

    // ---- details -----------------------------------------------------------

    pub fn details(&self, manga: &mut Manga, doc: &Document) {
        if let Some(t) = doc.select_first("h1, h2.widget-title").and_then(|e| e.text()) {
            let t = t.trim();
            if !t.is_empty() {
                manga.title = t.into();
            }
        }
        manga.cover = doc
            .select_first("img.img-responsive, .boxed img")
            .and_then(|i| self.img(&i))
            .or_else(|| manga.cover.clone());
        manga.description = doc
            .select_first("div.well")
            .and_then(|e| e.text())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if let Some(a) = self.dd_text(doc, self.cfg.label_author) {
            manga.authors = Some(alloc::vec![a]);
        }
        let tags: Vec<String> = self
            .dd(doc, self.cfg.label_tag)
            .and_then(|dd| dd.select("a").map(|l| l.filter_map(|x| x.text()).collect()))
            .unwrap_or_default();
        if !tags.is_empty() {
            manga.tags = Some(tags.into_iter().map(|t| t.trim().to_string()).collect());
        }
        manga.status = match self.dd_text(doc, self.cfg.label_state) {
            Some(s) => {
                let t = s.to_lowercase();
                if t.contains("complet") || t.contains("terminé") || t.contains("tamamlan") {
                    MangaStatus::Completed
                } else if t.contains("ongoing") || t.contains("en cours") || t.contains("devam") {
                    MangaStatus::Ongoing
                } else {
                    MangaStatus::Unknown
                }
            }
            None => MangaStatus::Unknown,
        };
        manga.url = Some(self.abs(&manga.key));
        manga.viewer = Viewer::Webtoon;
    }

    /// The value cell is the `<dd>` immediately after the labelled `<dt>`.
    /// kotatsu selects it with `dt:contains(Label)` + `nextElementSibling()`;
    /// `:contains()` isn't real CSS, so the label match happens in Rust.
    fn dd(&self, doc: &Document, label: &str) -> Option<Element> {
        if label.is_empty() {
            return None;
        }
        nyora_common::labelled_row(doc, "dt", &[label]).and_then(|dt| dt.next())
    }

    fn dd_text(&self, doc: &Document, label: &str) -> Option<String> {
        self.dd(doc, label)
            .and_then(|e| e.text())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    }

    // ---- chapters ----------------------------------------------------------

    pub fn chapters(&self, doc: &Document) -> Vec<Chapter> {
        // `ul.chapters > li:not(.btn)` — the :not() is filtered in Rust rather
        // than relied on, since selector-engine support for it varies.
        let Some(rows) = nyora_common::select_nonempty(doc, "ul.chapters > li") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        let mut index = 0f32;
        for row in rows.iter().rev() {
            if row
                .attr("class")
                .map(|c| c.split_whitespace().any(|x| x == "btn"))
                .unwrap_or(false)
            {
                continue;
            }
            let Some(a) = row.select_first("a") else { continue };
            let Some(href) = a.attr("href") else { continue };
            let key = self.rel(&href);
            if seen.iter().any(|s| s == &key) {
                continue;
            }
            seen.push(key.clone());
            index += 1.0;
            out.push(Chapter {
                key: key.clone(),
                title: row
                    .select_first("h5")
                    .and_then(|e| e.text())
                    .or_else(|| a.text())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                chapter_number: Some(index),
                date_uploaded: row
                    .select_first("div.date-chapter-title-rtl")
                    .and_then(|e| e.text())
                    .and_then(|d| date::parse(d.trim(), self.cfg.date_pattern, self.cfg.locale)),
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
        if let Some(imgs) = nyora_common::select_nonempty(doc, "div#all img") {
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
        let doc = self.get(&format!(
            "{}/{}/",
            self.cfg.base_url(),
            self.cfg.tag_url.trim_matches('/')
        ))?;
        let mut out = Vec::new();
        if let Some(items) = nyora_common::select_nonempty(&doc, "ul.list-category li a") {
            for a in items {
                let Some(href) = a.attr("href") else { continue };
                let key = href
                    .split("cat=")
                    .nth(1)
                    .map(|s| s.split('&').next().unwrap_or(s).to_string())
                    .unwrap_or_default();
                let title = a.text().unwrap_or_default().trim().to_string();
                if key.is_empty() || title.is_empty() || out.iter().any(|(_, k)| k == &key) {
                    continue;
                }
                out.push((title, key));
            }
        }
        Ok(out)
    }
}

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
