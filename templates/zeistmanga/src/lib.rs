//! ZeistManga theme template for Aidoku, ported from nyora-data-driven's
//! ZeistmangaEngine.kt. 50 kotatsu sources, and — unusually — 31 of them are
//! still live, making this the highest-value engine after madara/mangareader.
//!
//! These are Blogger/Blogspot sites, so browsing goes through the Blogger feed
//! JSON API rather than scraping HTML list pages. Details, chapters and pages
//! are still HTML.

#![no_std]
extern crate alloc;

pub mod config;

pub use config::ZeistConfig;
pub use nyora_common::date;

use aidoku::{
    alloc::{string::String, string::ToString, vec::Vec},
    imports::{
        html::{Document, Element},
        net::{HttpMethod, Request},
    },
    prelude::*,
    Chapter, ContentRating, Manga, MangaPageResult, MangaStatus, Page, PageContent, Result, Viewer,
};
use serde::Deserialize;

// ---- Blogger feed shapes ---------------------------------------------------

#[derive(Deserialize)]
struct Feed {
    feed: FeedBody,
}
#[derive(Deserialize)]
struct FeedBody {
    #[serde(default)]
    entry: Vec<Entry>,
}
#[derive(Deserialize)]
struct Entry {
    title: Text,
    #[serde(default)]
    link: Vec<Link>,
    #[serde(rename = "media$thumbnail", default)]
    thumbnail: Option<Thumb>,
    #[serde(default)]
    content: Option<Text>,
    #[serde(default)]
    published: Option<Text>,
}
#[derive(Deserialize)]
struct Text {
    #[serde(rename = "$t")]
    t: String,
}
#[derive(Deserialize)]
struct Link {
    #[serde(default)]
    rel: String,
    #[serde(default)]
    href: String,
}

impl Entry {
    /// Blogger emits several <link> rels per entry; `alternate` is the reader
    /// page. Falling back to the first link keeps entries usable on themes that
    /// omit the rel.
    fn alternate(&self) -> Option<&str> {
        self.link
            .iter()
            .find(|l| l.rel == "alternate")
            .or_else(|| self.link.first())
            .map(|l| l.href.as_str())
            .filter(|h| !h.is_empty())
    }
}
#[derive(Deserialize)]
struct Thumb {
    url: String,
}

pub struct ZeistSource {
    pub cfg: &'static ZeistConfig,
}

impl ZeistSource {
    pub const fn new(cfg: &'static ZeistConfig) -> Self {
        Self { cfg }
    }

    fn abs(&self, url: &str) -> String {
        if url.starts_with("http") {
            url.into()
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
                .unwrap_or_else(|| s.into())
        } else {
            format!("/{s}")
        }
    }

    // ---- listing (Blogger feed JSON) ---------------------------------------

    /// `start-index` is 1-based and counts entries, not pages. kotatsu also asks
    /// for one MORE than the page size and never drops the extra — preserved
    /// here so pagination lines up with the reference implementation.
    fn feed_url(&self, label: &str, page: i32, query: Option<&str>) -> String {
        let max = self.cfg.max_results;
        let start = max * (page.max(1) - 1) + 1;
        let mut u = format!(
            "{}/feeds/posts/default/-/{}?alt=json&orderby=published&max-results={}&start-index={}",
            self.cfg.base_url(),
            encode_path(label),
            max + 1,
            start
        );
        if let Some(q) = query.filter(|q| !q.is_empty()) {
            u.push_str(&format!(
                "&q=label:{}+{}",
                encode(self.cfg.manga_category),
                encode(q)
            ));
        }
        u
    }

    pub fn search_filtered(
        &self,
        query: Option<String>,
        page: i32,
        _order: Option<&str>,
        genre: Option<&str>,
    ) -> Result<MangaPageResult> {
        // A genre browses by its own Blogger label; otherwise the series label.
        let label = genre.unwrap_or(self.cfg.manga_category);
        let url = self.feed_url(label, page, query.as_deref());
        let feed: Feed = Request::new(&url, HttpMethod::Get)?.json_owned()?;
        let entries = self.parse_feed(feed);
        Ok(MangaPageResult {
            has_next_page: !entries.is_empty(),
            entries,
        })
    }

    pub fn search(&self, query: Option<String>, page: i32, order: Option<&str>) -> Result<MangaPageResult> {
        self.search_filtered(query, page, order, None)
    }

    /// The feed only supports `orderby=published`, so every listing is the same
    /// query with a different label. Status listings use the source's own
    /// localised status words as labels.
    pub fn listing(&self, id: &str, page: i32) -> Result<MangaPageResult> {
        let label = match id {
            "ongoing" => self.cfg.state_ongoing,
            "completed" => self.cfg.state_finished,
            _ => self.cfg.manga_category,
        };
        self.search_filtered(None, page, None, Some(label))
    }

    fn parse_feed(&self, feed: Feed) -> Vec<Manga> {
        let mut out = Vec::new();
        for e in feed.feed.entry {
            let Some(href) = e.alternate().map(|h| h.to_string()) else {
                continue;
            };
            let title = e.title.t.trim().to_string();
            if title.is_empty() {
                continue;
            }
            // Blogger thumbnails come back at a tiny crop size; rewriting the
            // size segment to w600 is what makes covers usable.
            let cover = e
                .thumbnail
                .map(|t| upsize(&t.url))
                .or_else(|| e.content.as_ref().and_then(|c| first_img(&c.t)));
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

    pub fn fetch_manga(&self, key: &str) -> Result<Document> {
        Ok(Request::new(self.abs(key), HttpMethod::Get)?.html()?)
    }
    pub fn fetch_chapter(&self, key: &str) -> Result<Document> {
        Ok(Request::new(self.abs(key), HttpMethod::Get)?.html()?)
    }

    // ---- details -----------------------------------------------------------

    pub fn details(&self, manga: &mut Manga, doc: &Document) {
        if let Some(t) = doc
            .select_first("h1.entry-title")
            .or_else(|| doc.select_first("h1"))
            .and_then(|e| e.text())
        {
            let t = t.trim();
            if !t.is_empty() {
                manga.title = t.into();
            }
        }
        manga.description = ["#synopsis", "#Sinopse", "#sinopas", ".sinopsis", ".sinopas"]
            .iter()
            .find_map(|s| doc.select_first(s))
            .and_then(|e| e.text())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        let tags: Vec<String> = nyora_common::select_nonempty(doc, self.cfg.tags_sel())
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

        if let Some(a) = self.labelled(doc, &["author", "الكاتب", "autor", "yazar"]) {
            manga.authors = Some(alloc::vec![a]);
        }
        manga.status = self.status(doc);
        manga.url = Some(self.abs(&manga.key));
        manga.viewer = Viewer::Webtoon;
    }

    /// kotatsu reaches these with `:contains(Status) .dt` chains, but
    /// `:contains()` is a Jsoup extension rather than CSS and isn't accepted by
    /// Aidoku's selector engine. Iterating the label rows is portable and copes
    /// with the Arabic/Spanish/Turkish labels these themes use.
    fn labelled(&self, doc: &Document, labels: &[&str]) -> Option<String> {
        for sel in ["div.y6x11p", "ul.infonime li", "dl", ".info-list li"] {
            let Some(rows) = nyora_common::select_nonempty(doc, sel) else {
                continue;
            };
            for row in rows {
                let text = row.text().unwrap_or_default();
                let lower = text.trim().to_lowercase();
                if lower.is_empty() || !labels.iter().any(|l| lower.contains(l)) {
                    continue;
                }
                let val = row
                    .select_first(".dt, span, dd")
                    .and_then(|e| e.text())
                    .filter(|v| {
                        let v = v.trim().to_lowercase();
                        !v.is_empty() && !labels.iter().any(|l| v == *l)
                    })
                    .or_else(|| row.children().last().and_then(|c| c.text()));
                if let Some(v) = val {
                    let v = v.trim().to_string();
                    if !v.is_empty() && !labels.iter().any(|l| v.to_lowercase() == *l) {
                        return Some(v);
                    }
                }
            }
        }
        None
    }

    fn status(&self, doc: &Document) -> MangaStatus {
        let raw = doc
            .select_first("span.status-novel, span[data-status]")
            .and_then(|e| e.text())
            .or_else(|| self.labelled(doc, &["status", "estado", "حالة"]))
            .unwrap_or_default()
            .trim()
            .to_lowercase();
        if raw.is_empty() {
            return MangaStatus::Unknown;
        }
        // The status words are per-source config, because these themes are
        // localised — matching English words alone would miss most sources.
        let eq = |w: &str| !w.is_empty() && raw.contains(&w.to_lowercase());
        if eq(self.cfg.state_finished) {
            MangaStatus::Completed
        } else if eq(self.cfg.state_ongoing) {
            MangaStatus::Ongoing
        } else if eq(self.cfg.state_abandoned) {
            MangaStatus::Cancelled
        } else if raw.contains("ongoing") {
            MangaStatus::Ongoing
        } else if raw.contains("complet") {
            MangaStatus::Completed
        } else {
            MangaStatus::Unknown
        }
    }

    // ---- chapters ----------------------------------------------------------

    /// Chapters live in a SECOND Blogger feed, keyed by a label the details page
    /// only exposes inside inline scripts. kotatsu probes five different places
    /// because the theme has changed shape over time; all five are reproduced.
    pub fn chapters(&self, doc: &Document) -> Result<Vec<Chapter>> {
        let Some(label) = self.chapter_label(doc) else {
            return Ok(Vec::new());
        };
        let url = format!(
            "{}/feeds/posts/default/-/{}?alt=json&orderby=published&max-results=9999",
            self.cfg.base_url(),
            encode_path(&label)
        );
        let feed: Feed = Request::new(&url, HttpMethod::Get)?.json_owned()?;
        let mut out: Vec<Chapter> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for e in feed.feed.entry.into_iter().rev() {
            let Some(href) = e.alternate().map(|h| h.to_string()) else {
                continue;
            };
            let key = self.rel(&href);
            if seen.iter().any(|s| s == &key) {
                continue;
            }
            seen.push(key.clone());
            let date_uploaded = e.published.as_ref().and_then(|p| {
                // "2024-03-01T09:00:00+07:00" -> take the date part
                let d = p.t.split('T').next().unwrap_or(&p.t);
                date::parse(d, "yyyy-MM-dd", self.cfg.locale)
            });
            out.push(Chapter {
                key: key.clone(),
                title: Some(e.title.t.trim().to_string()).filter(|s| !s.is_empty()),
                chapter_number: Some(out.len() as f32 + 1.0),
                date_uploaded,
                url: Some(self.abs(&key)),
                ..Default::default()
            });
        }
        out.reverse();
        Ok(out)
    }

    fn chapter_label(&self, doc: &Document) -> Option<String> {
        // 1) a feed <script src> whose path carries the label
        if let Some(s) = doc.select_first("#myUL script[src]").and_then(|e| e.attr("src")) {
            if let Some(after) = s.split("/-/").nth(1) {
                let l = after.split('?').next().unwrap_or(after);
                if !l.is_empty() {
                    return Some(decode(l));
                }
            }
        }
        // 2..5) inline scripts, each a different generation of the theme
        if let Some(scripts) = doc.select("script") {
            for sc in scripts {
                let body = script_body(&sc);
                for (marker, close) in [
                    ("label = '", "'"),
                    ("clwd.run('", "'"),
                    ("label_chapter = \"", "\""),
                ] {
                    if let Some(i) = body.find(marker) {
                        let rest = &body[i + marker.len()..];
                        if let Some(j) = rest.find(close) {
                            let v = rest[..j].trim();
                            if !v.is_empty() {
                                return Some(v.into());
                            }
                        }
                    }
                }
            }
        }
        doc.select_first("#chapterlist[data-post-title]")
            .and_then(|e| e.attr("data-post-title"))
    }

    // ---- pages -------------------------------------------------------------

    pub fn pages(&self, doc: &Document) -> Result<Vec<Page>> {
        let mk = |u: String| Page {
            content: PageContent::Url(u, None),
            ..Default::default()
        };
        if let Some(scripts) = doc.select("script") {
            for sc in scripts {
                let body = script_body(&sc);
                // theme A: chapterImage = ["…","…"]
                if let Some(i) = body.find("chapterImage") {
                    let rest = &body[i..];
                    if let (Some(a), Some(b)) = (rest.find('['), rest.find(']')) {
                        if b > a {
                            let urls: Vec<String> = rest[a + 1..b]
                                .split(',')
                                .map(|s| s.trim().trim_matches('"').trim().replace("\\/", "/"))
                                .filter(|s| !s.is_empty())
                                .collect();
                            if !urls.is_empty() {
                                return Ok(urls.into_iter().map(|u| mk(self.abs(&u))).collect());
                            }
                        }
                    }
                }
                // theme B: const content = `…<img src="…">…`
                if body.contains("const content") {
                    let urls: Vec<String> = body
                        .split("src=\"")
                        .skip(1)
                        .filter_map(|s| s.split('"').next())
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if !urls.is_empty() {
                        return Ok(urls.into_iter().map(|u| mk(self.abs(&u))).collect());
                    }
                }
            }
        }
        // theme C: plain <img> in the reader area
        let mut out = Vec::new();
        if let Some(imgs) = doc.select(self.cfg.page_sel()) {
            for img in imgs {
                if let Some(u) = nyora_common::img_src(&img, |x| self.abs(x)) {
                    out.push(mk(u));
                }
            }
        }
        Ok(out)
    }

    /// Genres, either from config or scraped from the theme's genre page.
    pub fn genres(&self) -> Result<Vec<(String, String)>> {
        if !self.cfg.static_tags.is_empty() {
            return Ok(self
                .cfg
                .static_tags
                .iter()
                .map(|(t, k)| ((*t).to_string(), (*k).to_string()))
                .collect());
        }
        let url = match self.cfg.tag_path {
            Some(p) => self.abs(p),
            None => self.cfg.base_url(),
        };
        let doc = Request::new(&url, HttpMethod::Get)?.html()?;
        let sel = self
            .cfg
            .tag_item
            .or(self.cfg.tag_root_sel)
            .unwrap_or("a[href*=label]");
        let mut out = Vec::new();
        if let Some(items) = nyora_common::select_nonempty(&doc, sel) {
            for a in items {
                let Some(href) = a.attr("href") else { continue };
                // key modes: the label sits either in ?q=label:X or after /label/
                let key = href
                    .split("label/")
                    .nth(1)
                    .map(|s| s.split('?').next().unwrap_or(s))
                    .or_else(|| href.split("label:").nth(1))
                    .map(|s| decode(s.trim_matches('/')))
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

/// Blogger serves thumbnails at a small crop; swap the size token for w600.
/// kotatsu uses a negative lookahead to find the LAST `=s` — Rust's regex crate
/// has no lookaround, so this finds it directly with rfind.
fn upsize(url: &str) -> String {
    if let Some(i) = url.rfind("=s") {
        let tail = &url[i..];
        if tail.ends_with("-c") || tail.ends_with("-c-rw") {
            return format!("{}=w600", &url[..i]);
        }
    }
    if let Some(i) = url.find("/s") {
        if let Some(j) = url[i + 1..].find("-c").map(|x| x + i + 1) {
            if let Some(end) = url[j..].find('/').map(|x| x + j) {
                return format!("{}/w600{}", &url[..i], &url[end..]);
            }
        }
    }
    url.into()
}

fn first_img(html: &str) -> Option<String> {
    let i = html.find("<img")?;
    let rest = &html[i..];
    let j = rest.find("src=\"")? + 5;
    let k = rest[j..].find('"')?;
    Some(rest[j..j + k].to_string())
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

/// Path segments keep `/` but must escape spaces and non-ASCII.
fn encode_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(*b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hi = (b[i + 1] as char).to_digit(16);
            let lo = (b[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(if b[i] == b'+' { b' ' } else { b[i] });
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.into())
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
