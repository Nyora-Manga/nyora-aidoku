//! AsuraScans template for Aidoku, ported from nyora-data-driven's
//! AsurascansEngine.kt. One source, Astro-rendered.
//!
//! Two things the Kotlin gets wrong today and this does not:
//!  - the domain. The data row says `asuracomic.net`, but that now 301s TO
//!    `asurascans.com` — the redirect reversed at some point.
//!  - the author and genre selectors, which target markup the site no longer
//!    emits (`div.grid > div:has(h3…)`, `button.text-white`). Both are now
//!    plain `/browse?…` links.
//!
//! The reader's page list is the interesting part: it lives in an Astro island's
//! serialised props rather than in any <img> tag.

#![no_std]
extern crate alloc;

pub mod config;

pub use config::AsuraConfig;
pub use nyora_common::date;

use aidoku::{
    alloc::{string::String, string::ToString, vec::Vec},
    imports::{
        html::Document,
        net::{HttpMethod, Request},
    },
    prelude::*,
    Chapter, ContentRating, Manga, MangaPageResult, MangaStatus, Page, PageContent, Result, Viewer,
};

/// Chapters published within this window are hidden. Leaked or scheduled
/// chapters appear briefly with a future-ish timestamp and then settle; the
/// reference implementation drops them rather than show a broken entry.
const HIDE_RECENT: i64 = 6 * 60 * 60;

pub struct AsuraSource {
    pub cfg: &'static AsuraConfig,
}

impl AsuraSource {
    pub const fn new(cfg: &'static AsuraConfig) -> Self {
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
                .unwrap_or_else(|| "/".into())
        } else {
            format!("/{s}")
        }
    }

    fn get(&self, url: &str) -> Result<Document> {
        Ok(Request::new(url, HttpMethod::Get)?.html()?)
    }

    // ---- listing -----------------------------------------------------------

    /// One grammar covers browse, search and filtering. `sort` is always sent,
    /// with an EMPTY value meaning "recently updated" — that is the site's
    /// default, not a missing parameter.
    pub fn search_filtered(
        &self,
        query: Option<String>,
        page: i32,
        order: Option<&str>,
        genre: Option<&str>,
    ) -> Result<MangaPageResult> {
        let mut url = format!("{}/browse?page={}", self.cfg.base_url(), page.max(1));
        if let Some(q) = query.as_deref().filter(|q| !q.is_empty()) {
            url.push_str(&format!("&search={}", encode(q)));
        }
        if let Some(g) = genre {
            url.push_str(&format!("&genres={}", encode(g)));
        }
        url.push_str(&format!("&sort={}", order.unwrap_or("")));

        let doc = self.get(&url)?;
        let entries = self.parse_list(&doc);
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
            "popular" => "popular",
            "rating" => "rating",
            "alphabetical" => "asc",
            _ => "", // latest / recently updated
        };
        self.search_filtered(None, page, Some(order), None)
    }

    pub fn parse_list(&self, doc: &Document) -> Vec<Manga> {
        let mut out = Vec::new();
        let Some(cards) = nyora_common::select_nonempty(doc, "#series-grid .series-card") else {
            return out;
        };
        for card in cards {
            // NB: the attribute value MUST be quoted. `a[href*=/comics/]`
            // parses to zero matches — the unquoted slashes break the selector.
            let Some(a) = card
                .select_first("a[href*=\"/comics/\"]")
                .or_else(|| card.select_first("a"))
            else {
                continue;
            };
            let Some(href) = a.attr("href") else { continue };
            let title = card
                .select_first("h3")
                .and_then(|e| e.text())
                .unwrap_or_default()
                .trim()
                .to_string();
            if title.is_empty() {
                continue;
            }
            // Status is the LAST span in the meta row — the earlier ones are
            // chapter counts and type.
            let status = card
                .select("div.p-3 span")
                .and_then(|l| l.collect::<Vec<_>>().last().and_then(|e| e.text()))
                .map(|s| self.status(&s))
                .unwrap_or(MangaStatus::Unknown);
            out.push(Manga {
                key: self.rel(&href),
                title,
                cover: card.select_first("img").and_then(|i| nyora_common::img_src(&i, |u| self.abs(u))),
                status,
                content_rating: if self.cfg.nsfw { ContentRating::NSFW } else { ContentRating::Safe },
                ..Default::default()
            });
        }
        out
    }

    fn status(&self, raw: &str) -> MangaStatus {
        match raw.trim().to_lowercase().as_str() {
            s if s.contains("ongoing") => MangaStatus::Ongoing,
            s if s.contains("completed") => MangaStatus::Completed,
            s if s.contains("hiatus") => MangaStatus::Hiatus,
            s if s.contains("dropped") || s.contains("cancel") => MangaStatus::Cancelled,
            _ => MangaStatus::Unknown,
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
        if let Some(t) = doc.select_first("article h1").and_then(|e| e.text()) {
            let t = t.trim();
            if !t.is_empty() {
                manga.title = t.into();
            }
        }
        manga.cover = doc
            .select_first("article img")
            .and_then(|i| nyora_common::img_src(&i, |u| self.abs(u)))
            .or_else(|| manga.cover.clone());

        let desc = doc
            .select_first("#description-text")
            .or_else(|| doc.select_first("span.font-medium.text-sm"))
            .and_then(|e| e.text())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let alts = doc
            .select_first("#alt-titles")
            .and_then(|e| e.text())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && s != "N/A");
        manga.description = match (alts, desc) {
            (Some(a), Some(d)) => Some(format!("Alternative: {a}\n\n{d}")),
            (Some(a), None) => Some(format!("Alternative: {a}")),
            (None, d) => d,
        };

        // The reference selectors for these two target markup the site no
        // longer emits; both are plain browse links now.
        let tags: Vec<String> = nyora_common::select_nonempty(doc, "a[href*=\"/browse?genres=\"]")
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
        let authors: Vec<String> = nyora_common::select_nonempty(doc, "a[href*=\"/browse?author=\"]")
            .map(|l| l.iter().filter_map(|e| e.text()).map(|t| t.trim().to_string()).collect())
            .unwrap_or_default();
        if !authors.is_empty() {
            manga.authors = Some(authors);
        }
        let artists: Vec<String> = nyora_common::select_nonempty(doc, "a[href*=\"/browse?artist=\"]")
            .map(|l| l.iter().filter_map(|e| e.text()).map(|t| t.trim().to_string()).collect())
            .unwrap_or_default();
        if !artists.is_empty() {
            manga.artists = Some(artists);
        }

        if let Some(s) = doc
            .select_first("span.text-base.font-bold.capitalize")
            .and_then(|e| e.text())
        {
            let st = self.status(&s);
            if st != MangaStatus::Unknown {
                manga.status = st;
            }
        }
        manga.url = Some(self.abs(&manga.key));
        manga.viewer = Viewer::Webtoon;
    }

    // ---- chapters ----------------------------------------------------------

    pub fn chapters(&self, doc: &Document) -> Vec<Chapter> {
        let Some(rows) = nyora_common::select_nonempty(doc, "a.group[href*=\"/chapter/\"]") else {
            return Vec::new();
        };
        let now = aidoku::imports::std::current_date();
        let mut out = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        let mut index = 0f32;
        for a in rows.iter().rev() {
            let Some(href) = a.attr("href") else { continue };
            let key = self.rel(&href);
            if seen.iter().any(|s| s == &key) {
                continue;
            }

            let label = a.select_first("span.font-medium").and_then(|e| e.text()).unwrap_or_default();
            let date_uploaded = a
                .select_first("span.text-sm.text-white\\/40")
                .and_then(|e| e.text())
                .and_then(|d| self.parse_date(d.trim(), now));

            // Skip very recent uploads: they are usually scheduled or leaked
            // entries that aren't actually readable yet.
            if let Some(t) = date_uploaded {
                if now - t < HIDE_RECENT {
                    continue;
                }
            }
            seen.push(key.clone());
            index += 1.0;

            out.push(Chapter {
                key: key.clone(),
                chapter_number: Some(chapter_number(&label).unwrap_or(index)),
                title: a
                    .select_first("span.text-sm.text-white\\/50")
                    .and_then(|e| e.text())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                date_uploaded,
                url: Some(self.abs(&key)),
                ..Default::default()
            });
        }
        out.reverse();
        out
    }

    fn parse_date(&self, raw: &str, now: i64) -> Option<i64> {
        let s = raw.trim().to_lowercase();
        if s.is_empty() {
            return None;
        }
        if s.contains("last week") {
            return Some(now - 7 * 86400);
        }
        if s.contains("yesterday") {
            return Some(now - 86400);
        }
        if s.ends_with(" ago") {
            let n: i64 = s.chars().take_while(|c| c.is_ascii_digit()).collect::<String>().parse().ok()?;
            let unit = if s.contains("second") { 1 }
                else if s.contains("min") { 60 }
                else if s.contains("hour") { 3600 }
                else if s.contains("day") { 86400 }
                else if s.contains("week") { 7 * 86400 }
                else if s.contains("month") { 30 * 86400 }
                else if s.contains("year") { 365 * 86400 }
                else { return None };
            return Some(now - n * unit);
        }
        // "March 3rd, 2024" -> strip the ordinal suffix before parsing
        date::parse(&strip_ordinals(raw), "MMM d, yyyy", self.cfg.locale)
    }

    // ---- pages -------------------------------------------------------------

    /// Page URLs are not in the DOM as images — they're inside the serialised
    /// props of the reader's Astro island, as `"url":[0,"https://…"]`.
    pub fn pages(&self, doc: &Document) -> Result<Vec<Page>> {
        let props = doc
            .select_first("astro-island[component-url*='ChapterReader']")
            .and_then(|e| e.attr("props"))
            .unwrap_or_default();
        let mut out = Vec::new();
        for u in extract_urls(&props) {
            out.push(Page {
                content: PageContent::Url(u, None),
                ..Default::default()
            });
        }
        if !out.is_empty() {
            return Ok(out);
        }
        // Fallback: some chapters render plain images.
        if let Some(imgs) = nyora_common::select_nonempty(doc, "img[src*=\"chapters\"]") {
            for img in imgs {
                if let Some(u) = nyora_common::img_src(&img, |x| self.abs(x)) {
                    out.push(Page { content: PageContent::Url(u, None), ..Default::default() });
                }
            }
        }
        Ok(out)
    }

    pub fn genres(&self) -> Result<Vec<(String, String)>> {
        // The browse page renders genres client-side, so they can't be scraped;
        // the list is compiled in.
        Ok(self
            .cfg
            .genres
            .iter()
            .map(|g| ((*g).to_string(), slugify(g)))
            .collect())
    }
}

/// Pull page URLs out of the island props, in document order.
///
/// The attribute may arrive HTML-encoded or already decoded depending on the
/// host's parser, so both are handled rather than assuming one.
fn extract_urls(props: &str) -> Vec<String> {
    const MARKER: &str = "\"url\":[0,\"";
    let mut hay = alloc::string::String::from(props);
    if !hay.contains(MARKER) {
        hay = hay.replace("&quot;", "\"");
    }
    let mut out = Vec::new();
    let mut rest = hay.as_str();
    while let Some(i) = rest.find(MARKER) {
        rest = &rest[i + MARKER.len()..];
        if let Some(j) = rest.find('"') {
            let u = &rest[..j];
            if !u.is_empty() {
                out.push(u.replace("\\/", "/"));
            }
            rest = &rest[j..];
        } else {
            break;
        }
    }
    out
}

fn chapter_number(label: &str) -> Option<f32> {
    let l = label.to_lowercase();
    let i = l.find("chapter")? + "chapter".len();
    let s: String = l[i..]
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    s.parse().ok()
}

fn strip_ordinals(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < b.len() {
        out.push(b[i]);
        if b[i].is_ascii_digit() && i + 2 < b.len() {
            let two: String = b[i + 1..i + 3].iter().collect::<String>().to_lowercase();
            if matches!(two.as_str(), "st" | "nd" | "rd" | "th") {
                i += 3;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn slugify(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut dash = false;
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').to_string()
}

fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
