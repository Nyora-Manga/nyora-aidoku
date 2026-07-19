//! MangaReader / MangaThemesia theme template for Aidoku, ported from
//! nyora-data-driven's MangaReaderEngine.kt. 265 kotatsu sources — the second
//! largest family after Madara.
//!
//! Parsing is entirely on-device; the only traffic is to the source site.

#![no_std]
extern crate alloc;

pub mod config;

pub use config::MangaReaderConfig;
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

pub struct MangaReaderSource {
    pub cfg: &'static MangaReaderConfig,
}

impl MangaReaderSource {
    pub const fn new(cfg: &'static MangaReaderConfig) -> Self {
        Self { cfg }
    }

    fn abs(&self, url: &str) -> String {
        if url.starts_with("http") {
            url.into()
        } else if let Some(rest) = url.strip_prefix("//") {
            format!("https://{rest}")
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
            let after = s
                .split_once("://")
                .and_then(|(_, r)| r.split_once('/'))
                .map(|(_, p)| p)
                .unwrap_or(s);
            format!("/{after}")
        } else {
            format!("/{s}")
        }
    }

    fn img(&self, el: &Element) -> Option<String> {
        nyora_common::img_src(el, |u| self.abs(u))
    }

    // ---- listing -----------------------------------------------------------

    /// Two URL grammars, per MangaReaderEngine.kt:161-238:
    ///   search → /page/{n}/?s={query}
    ///   browse → {listUrl}/?order={token}&page={n}
    /// Both are 1-based; Aidoku already hands us 1-based pages, so no offset.
    pub fn search(
        &self,
        query: Option<String>,
        page: i32,
        order: Option<&str>,
        genre: Option<&str>,
    ) -> Result<MangaPageResult> {
        let p = page.max(1);
        let url = if let Some(q) = query.as_deref().filter(|q| !q.is_empty()) {
            format!("{}/page/{}/?s={}", self.cfg.base_url(), p, encode(q))
        } else {
            let mut u = format!("{}{}/?", self.cfg.base_url(), self.cfg.list_url);
            if let Some(o) = order {
                u.push_str(&format!("order={o}&"));
            }
            if let Some(g) = genre {
                // the only param the engine URL-encodes: genre[] -> genre%5B%5D
                u.push_str(&format!("genre%5B%5D={}&", encode(g)));
            }
            u.push_str(&format!("page={p}"));
            u
        };
        let doc = Request::new(&url, HttpMethod::Get)?.html()?;
        let entries = self.parse_list(&doc);
        Ok(MangaPageResult {
            has_next_page: !entries.is_empty(),
            entries,
        })
    }

    /// Same signature as the Madara template's, so the generated source body
    /// is engine-agnostic.
    pub fn search_filtered(
        &self,
        query: Option<String>,
        page: i32,
        order: Option<&str>,
        genre: Option<&str>,
    ) -> Result<MangaPageResult> {
        self.search(query, page, order, genre)
    }

    pub fn fetch_manga(&self, key: &str) -> Result<Document> {
        Ok(Request::new(&self.abs(key), HttpMethod::Get)?.html()?)
    }

    pub fn fetch_chapter(&self, key: &str) -> Result<Document> {
        Ok(Request::new(&self.abs(key), HttpMethod::Get)?.html()?)
    }

    pub fn listing(&self, id: &str, page: i32) -> Result<MangaPageResult> {
        let order = match id {
            "latest" => "update",
            "new" => "latest",
            "alphabetical" => "title",
            _ => "popular",
        };
        self.search(None, page, Some(order), None)
    }

    pub fn parse_list(&self, doc: &Document) -> Vec<Manga> {
        let mut out = Vec::new();
        let Some(cards) = doc.select(self.cfg.manga_list_sel()) else {
            return out;
        };
        for card in cards {
            let Some(a) = card.select_first("a") else { continue };
            let Some(href) = a.attr("href") else { continue };
            let title = card
                .select_first(self.cfg.manga_list_title_sel())
                .and_then(|t| t.text())
                .or_else(|| a.attr("title"))
                .unwrap_or_default()
                .trim()
                .to_string();
            if title.is_empty() {
                continue;
            }
            let cover = card
                .select_first(self.cfg.manga_list_img_sel())
                .or_else(|| card.select_first("img"))
                .and_then(|i| self.img(&i));
            out.push(Manga {
                key: self.rel(&href),
                title,
                cover,
                content_rating: if self.cfg.nsfw {
                    ContentRating::NSFW
                } else {
                    ContentRating::Safe
                },
                ..Default::default()
            });
        }
        out
    }

    // ---- details -----------------------------------------------------------

    pub fn details(&self, manga: &mut Manga, doc: &Document) {
        // 6-way title chain (MangaReaderEngine.kt:429-435): themes disagree on
        // where the title lives, and `<title>` is the last resort because it
        // carries the site name after " - ".
        let title = doc
            .select_first("h1.entry-title")
            .and_then(|e| e.text())
            .or_else(|| doc.select_first(".entry-title").and_then(|e| e.text()))
            .or_else(|| doc.select_first(".seriestucontent h1").and_then(|e| e.text()))
            .or_else(|| doc.select_first(".postbody h1").and_then(|e| e.text()))
            .or_else(|| doc.select_first("h1").and_then(|e| e.text()))
            .or_else(|| {
                doc.select_first("title")
                    .and_then(|e| e.text())
                    .map(|t| t.split(" - ").next().unwrap_or(&t).to_string())
            });
        if let Some(t) = title {
            let t = t.trim();
            if !t.is_empty() {
                manga.title = t.into();
            }
        }

        manga.cover = doc
            .select_first(".thumb img, .seriestucontent img, div.thumbook img")
            .and_then(|i| self.img(&i))
            .or_else(|| manga.cover.clone());

        manga.description = doc
            .select_first(self.cfg.description_sel())
            .and_then(|e| e.text())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        // Genres: table layout first, then the flat variant.
        let tags: Vec<String> = [".seriestugenre > a", ".wd-full .mgen > a", ".mgen > a"]
            .iter()
            .find_map(|s| nyora_common::select_nonempty(doc, s))
            .map(|l| {
                l.iter()
                    .filter_map(|e| e.text())
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty())
                    .collect()
            })
            .unwrap_or_default();
        if !tags.is_empty() {
            manga.tags = Some(tags);
        }

        if let Some(a) = self.labelled_value(doc, &["author", "auteur", "yazar", "artist"]) {
            manga.authors = Some(alloc::vec![a]);
        }
        manga.status = self.status(doc);

        // Explicit adult markers override the source-level flag.
        if doc
            .select_first(".restrictcontainer, .info-right .alr, .postbody .alr")
            .is_some()
        {
            manga.content_rating = ContentRating::NSFW;
        }
        manga.url = Some(self.abs(&manga.key));
        manga.viewer = Viewer::Webtoon;
    }

    /// Both detail layouts store metadata as label/value pairs — a table
    /// (`.infotable tr`) or divs (`.tsinfo .imptdt`). kotatsu selects these with
    /// `:contains(Label)` plus `lastElementSibling()`, but `:contains()` is a
    /// Jsoup extension rather than real CSS and isn't accepted by every HTML
    /// engine. Iterating and comparing text is portable and handles the
    /// non-Latin labels cleanly.
    fn labelled_value(&self, doc: &Document, labels: &[&str]) -> Option<String> {
        for sel in [".infotable tr", ".tsinfo .imptdt", ".tsinfo div", ".fmed"] {
            let Some(rows) = nyora_common::select_nonempty(doc, sel) else { continue };
            for row in rows {
                let text = row.text().unwrap_or_default();
                let lower = text.trim().to_lowercase();
                if lower.is_empty() || !labels.iter().any(|l| lower.starts_with(l) || lower.contains(l)) {
                    continue;
                }
                // value = last child element, falling back to the row text with
                // the label stripped off the front.
                let children = row.children();
                let val = children
                    .last()
                    .and_then(|c| c.text())
                    .filter(|v| {
                        let v = v.trim().to_lowercase();
                        !v.is_empty() && !labels.iter().any(|l| &v == l)
                    })
                    .or_else(|| {
                        let mut t = text.trim().to_string();
                        for l in labels {
                            if t.to_lowercase().starts_with(l) {
                                t = t[l.len()..].trim_start_matches([':', ' ']).to_string();
                                break;
                            }
                        }
                        Some(t)
                    });
                if let Some(v) = val {
                    let v = v.trim().to_string();
                    if !v.is_empty() {
                        return Some(v);
                    }
                }
            }
        }
        None
    }

    fn status(&self, doc: &Document) -> MangaStatus {
        let Some(raw) = self.labelled_value(doc, &["status", "durum", "estado", "statut"]) else {
            return MangaStatus::Unknown;
        };
        let t = raw.to_lowercase();
        if t.contains("complet") || t.contains("tamamland") || t.contains("finished") {
            MangaStatus::Completed
        } else if t.contains("ongoing") || t.contains("devam") || t.contains("en cours") {
            MangaStatus::Ongoing
        } else if t.contains("hiatus") || t.contains("pause") {
            MangaStatus::Hiatus
        } else if t.contains("cancel") || t.contains("drop") {
            MangaStatus::Cancelled
        } else {
            MangaStatus::Unknown
        }
    }

    // ---- chapters ----------------------------------------------------------

    /// Chapters are inline on the details page — unlike Madara there is no ajax
    /// fallback, so this needs no extra request. Rows are newest-first, and the
    /// counter advances only on kept rows so dedupe can't leave numbering gaps.
    pub fn chapters(&self, doc: &Document) -> Vec<Chapter> {
        let Some(rows) = doc.select(self.cfg.chapter_sel()) else {
            return Vec::new();
        };
        let rows: Vec<Element> = rows.collect();
        let mut seen: Vec<String> = Vec::new();
        let mut out: Vec<Chapter> = Vec::new();
        let mut index = 0f32;
        for row in rows.iter().rev() {
            let Some(a) = row.select_first("a") else { continue };
            let Some(href) = a.attr("href") else { continue };
            let key = self.rel(&href);
            if seen.iter().any(|s| s == &key) {
                continue;
            }
            seen.push(key.clone());
            index += 1.0;

            let title = row
                .select_first(".chapternum")
                .and_then(|e| e.text())
                .or_else(|| a.own_text())
                .map(|s| squash(&s))
                .filter(|s| !s.is_empty());
            let date_uploaded = row
                .select_first(".chapterdate")
                .and_then(|e| e.text())
                .and_then(|d| {
                    date::parse(
                        d.trim(),
                        &self.cfg.normalized_date_pattern(),
                        self.cfg.locale,
                    )
                });

            out.push(Chapter {
                key,
                title,
                chapter_number: Some(index),
                date_uploaded,
                url: Some(self.abs(&href)),
                ..Default::default()
            });
        }
        out.reverse();
        out
    }

    // ---- pages -------------------------------------------------------------

    /// MangaThemesia embeds the page list as JSON inside `ts_reader.run({...})`
    /// rather than emitting <img> tags, so scraping the reader area alone finds
    /// nothing on most sites. Try the script first, fall back to <img>.
    pub fn pages(&self, doc: &Document) -> Result<Vec<Page>> {
        if let Some(urls) = self.pages_from_script(doc) {
            if !urls.is_empty() {
                return Ok(urls
                    .into_iter()
                    .map(|u| Page {
                        content: PageContent::Url(u, None),
                        ..Default::default()
                    })
                    .collect());
            }
        }
        let mut out = Vec::new();
        if let Some(imgs) = doc.select(self.cfg.page_sel()) {
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

    fn pages_from_script(&self, doc: &Document) -> Option<Vec<String>> {
        let scripts = doc.select("script")?;
        for s in scripts {
            let body = script_body(&s);
            if !body.contains("ts_reader") {
                continue;
            }
            // The engine takes everything between the FIRST '(' and the LAST
            // ')' — the argument to ts_reader.run(...).
            let start = body.find('(')? + 1;
            let end = body.rfind(')')?;
            if end <= start {
                continue;
            }
            let json = &body[start..end];
            let urls = extract_images(json);
            if !urls.is_empty() {
                return Some(urls);
            }
        }
        None
    }
}

/// Pull `"images":[ "...", ... ]` out of the ts_reader blob.
///
/// Done by hand rather than with a JSON parser because the blob also carries
/// `"source"` URLs and other arrays we must not pick up, and a full parse would
/// need the whole object to be well-formed — which it isn't always, since some
/// themes inject trailing commas.
fn extract_images(json: &str) -> Vec<String> {
    let mut out = Vec::new();
    let Some(idx) = json.find("\"images\"") else {
        return out;
    };
    let rest = &json[idx..];
    let Some(open) = rest.find('[') else { return out };
    let Some(close) = rest[open..].find(']') else {
        return out;
    };
    for raw in rest[open + 1..open + close].split(',') {
        let t = raw.trim().trim_matches('"').trim();
        if t.is_empty() {
            continue;
        }
        out.push(t.replace("\\/", "/"));
    }
    out
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

/// Collapse runs of whitespace. MangaThemesia indents chapter titles with tabs
/// and newlines, which otherwise surface verbatim as "Chapter\t\t\t21.2".
fn squash(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut sp = false;
    for ch in s.trim().chars() {
        if ch.is_whitespace() {
            sp = true;
        } else {
            if sp && !out.is_empty() { out.push(' '); }
            sp = false;
            out.push(ch);
        }
    }
    out
}

/// A <script> body is a DataNode, so `text()` is empty and `html()` is
/// unreliable across HTML engines — `data()` is the accessor that actually
/// returns the source. Getting this wrong silently yields zero chapters/pages.
fn script_body(el: &Element) -> String {
    // Each accessor returns Some("") rather than None on the engine that
    // doesn't support it, so an .or_else() chain alone silently stops at the
    // first one and yields nothing. Filter the empties.
    [el.data(), el.html(), el.text()]
        .into_iter()
        .flatten()
        .find(|s| !s.trim().is_empty())
        .unwrap_or_default()
}
