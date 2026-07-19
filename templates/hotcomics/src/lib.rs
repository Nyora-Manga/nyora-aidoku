//! HotComics / TooMics theme template for Aidoku, ported from
//! nyora-data-driven's HotcomicsEngine.kt. 13 kotatsu sources.
//!
//! Structurally the closest of the small engines to mangareader: nested
//! selector config, chapter list inline on the details page, plain <img> page
//! scraping. The departures are `langPath` composition and a chapter list that
//! may hide its URLs behind a JS login popup.

#![no_std]
extern crate alloc;

pub mod config;

pub use config::HotComicsConfig;
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

/// The site serves a desktop layout only; a mobile UA gets a different DOM.
const UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 \
(KHTML, like Gecko) Chrome/120.0 Safari/537.36";

pub struct HotComicsSource {
    pub cfg: &'static HotComicsConfig,
}

impl HotComicsSource {
    pub const fn new(cfg: &'static HotComicsConfig) -> Self {
        Self { cfg }
    }

    /// Deliberately NO Referer. The reference implementation sets one on the
    /// details request, but sending it to toomics.com returns a page with an
    /// empty episode list — measured: 100 chapter rows without it, 0 with it.
    /// The desktop User-Agent, on the other hand, IS required; a mobile UA gets
    /// a different DOM.
    fn get(&self, url: &str) -> Result<Document> {
        Ok(Request::new(url, HttpMethod::Get)?
            .header("User-Agent", UA)
            .html()?)
    }

    fn abs(&self, key: &str) -> String {
        if key.starts_with("http") {
            key.into()
        } else {
            format!("{}{}", self.cfg.base_url(), key)
        }
    }

    /// Keys are stored WITHOUT the language segment, so the same title in
    /// /en and /de resolves to one entry in the library. The segment is put
    /// back by `abs()` at fetch time.
    fn strip_lang(&self, href: &str) -> String {
        let path = href
            .split_once("://")
            .and_then(|(_, r)| r.split_once('/'))
            .map(|(_, p)| p)
            .unwrap_or_else(|| href.trim_start_matches('/'));
        match path.split_once('/') {
            Some((_lang, rest)) => format!("/{rest}"),
            None => format!("/{path}"),
        }
    }

    fn img(&self, el: &Element) -> Option<String> {
        nyora_common::img_src(el, |u| {
            if u.starts_with("http") || u.starts_with("//") {
                if let Some(r) = u.strip_prefix("//") {
                    format!("https://{r}")
                } else {
                    u.into()
                }
            } else {
                self.abs(u)
            }
        })
    }

    // ---- listing -----------------------------------------------------------

    pub fn search_filtered(
        &self,
        query: Option<String>,
        page: i32,
        _order: Option<&str>,
        genre: Option<&str>,
    ) -> Result<MangaPageResult> {
        let p = page.max(1);
        // Browse renders everything on one page, so a second page is empty
        // rather than a repeat of the first.
        if self.cfg.one_page && p > 1 && query.is_none() {
            return Ok(MangaPageResult {
                entries: Vec::new(),
                has_next_page: false,
            });
        }
        let url = match query.as_deref().filter(|q| !q.is_empty()) {
            Some(q) if self.cfg.search_supported => {
                format!("{}/search?keyword={}&page={}", self.cfg.base_url(), encode(q), p)
            }
            Some(_) => {
                // Search is disabled on most of this family; returning empty is
                // honest, and better than silently browsing instead.
                return Ok(MangaPageResult {
                    entries: Vec::new(),
                    has_next_page: false,
                });
            }
            None => {
                let mut u = format!("{}{}", self.cfg.base_url(), self.cfg.mangas_url);
                if let Some(g) = genre {
                    u.push_str(&format!("/{g}"));
                }
                if !self.cfg.one_page {
                    u.push_str(&format!("?page={p}"));
                }
                u
            }
        };
        let doc = self.get(&url)?;
        let entries = self.parse_list(&doc);
        Ok(MangaPageResult {
            has_next_page: !entries.is_empty() && !self.cfg.one_page,
            entries,
        })
    }

    pub fn search(&self, query: Option<String>, page: i32, order: Option<&str>) -> Result<MangaPageResult> {
        self.search_filtered(query, page, order, None)
    }

    /// The site exposes no sort orders — popular and latest are the same list.
    pub fn listing(&self, _id: &str, page: i32) -> Result<MangaPageResult> {
        self.search_filtered(None, page, None, None)
    }

    pub fn parse_list(&self, doc: &Document) -> Vec<Manga> {
        // Completion is marked once for the whole page rather than per row.
        let all_finished = doc.select_first(".ico_fin").is_some();
        let mut out = Vec::new();
        let Some(rows) = nyora_common::select_nonempty(doc, self.cfg.mangas_sel()) else {
            return out;
        };
        for row in rows {
            let Some(a) = row.select_first("a").or_else(|| row.parent().and_then(|p| p.select_first("a")))
            else {
                continue;
            };
            let Some(href) = a.attr("href") else { continue };
            let title = row
                .select_first(".title")
                .and_then(|e| e.text())
                .unwrap_or_default()
                .trim()
                .to_string();
            if title.is_empty() {
                continue;
            }
            let cover = row.select_first("img").and_then(|i| self.img(&i));
            let authors: Vec<String> = row
                .select_first(".writer")
                .and_then(|e| e.text())
                .map(|s| alloc::vec![s.trim().to_string()])
                .unwrap_or_default();
            let adult = row.select_first(".ico-18plus").is_some();
            out.push(Manga {
                key: self.strip_lang(&href),
                title,
                cover,
                authors: if authors.is_empty() { None } else { Some(authors) },
                status: if all_finished {
                    MangaStatus::Completed
                } else {
                    MangaStatus::Ongoing
                },
                content_rating: if adult || self.cfg.nsfw {
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
        self.get(&self.abs(key))
    }
    pub fn fetch_chapter(&self, key: &str) -> Result<Document> {
        self.get(&self.abs(key))
    }

    // ---- details -----------------------------------------------------------

    /// The details page adds only a synopsis — everything else was already on
    /// the list card, so existing fields are left untouched rather than being
    /// overwritten with blanks.
    pub fn details(&self, manga: &mut Manga, doc: &Document) {
        if let Some(d) = doc
            .select_first("div.title_content_box h2, .episode_area .desc, p[itemprop*=description]")
            .and_then(|e| e.text())
        {
            let d = d.trim();
            if !d.is_empty() {
                manga.description = Some(d.into());
            }
        }
        if manga.title.is_empty() {
            if let Some(t) = doc.select_first("h1, .title").and_then(|e| e.text()) {
                manga.title = t.trim().into();
            }
        }
        manga.url = Some(self.abs(&manga.key));
        manga.viewer = Viewer::Webtoon;
    }

    // ---- chapters ----------------------------------------------------------

    pub fn chapters(&self, doc: &Document) -> Vec<Chapter> {
        if self.cfg.popup_login_chapters {
            return self.chapters_popup(doc);
        }
        let sel = self.cfg.chapters_sel();
        // `li.normal_ep:has(.coin-type1)` is the shipped selector on every
        // TooMics row, but `:has()` is a Jsoup extension the Aidoku selector
        // engine rejects. Split it: select the element part, then filter on the
        // child in Rust.
        let (base, requires) = match sel.split_once(":has(") {
            Some((b, rest)) => (b, rest.strip_suffix(')')),
            None => (sel, None),
        };
        let Some(rows) = nyora_common::select_nonempty(doc, base) else {
            return Vec::new();
        };
        // `:has(.coin-type1)` marks coin-gated episodes. On the live markup it
        // matches 2 of 100 rows and none of those 2 carry a usable URL, so
        // applying it strictly throws away the entire chapter list. Build from
        // the filtered set, and if that yields nothing usable, build from all
        // rows instead — the marker is advisory, the URL is what matters.
        let filtered: Vec<&Element> = match requires {
            Some(req) => rows.iter().filter(|r| r.select_first(req).is_some()).collect(),
            None => rows.iter().collect(),
        };
        let out = self.collect_chapters(&filtered);
        if !out.is_empty() {
            return out;
        }
        let all: Vec<&Element> = rows.iter().collect();
        self.collect_chapters(&all)
    }

    fn collect_chapters(&self, rows: &[&Element]) -> Vec<Chapter> {
        let mut out = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            let Some(a) = row.select_first("a") else { continue };
            let Some(href) = a
                .attr("href")
                .filter(|h| !h.starts_with("javascript"))
                .or_else(|| a.attr("onclick").and_then(|o| between(&o, "location.href='", "'")))
                .or_else(|| a.attr("onclick").and_then(|o| between(&o, "href='", "'")))
                .or_else(|| row.attr("onclick").and_then(|o| between(&o, "location.href='", "'")))
            else {
                continue;
            };
            let key = self.strip_lang(&href);
            if seen.iter().any(|s| s == &key) {
                continue;
            }
            seen.push(key.clone());
            let number = row
                .select_first(".num, .cell-num")
                .and_then(|e| e.text())
                .and_then(|t| {
                    t.trim()
                        .trim_start_matches(|c: char| !c.is_ascii_digit())
                        .split_whitespace()
                        .next()
                        .and_then(|x| x.parse::<f32>().ok())
                })
                .unwrap_or((i + 1) as f32);
            let date_uploaded = row
                .select_first("time[datetime]")
                .and_then(|e| e.attr("datetime"))
                .or_else(|| row.select_first(".cell-time").and_then(|e| e.text()))
                .and_then(|d| self.parse_date(d.trim()));
            out.push(Chapter {
                key: key.clone(),
                title: row
                    .select_first(".title, .cell-title, .cell-num")
                    .and_then(|e| e.text())
                    .or_else(|| a.own_text())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                chapter_number: Some(number),
                date_uploaded,
                url: Some(self.abs(&key)),
                ..Default::default()
            });
        }
        out.reverse();
        out
    }

    /// Some sites gate chapters behind a login popup: the href is a JS call and
    /// the real path is its argument.
    fn chapters_popup(&self, doc: &Document) -> Vec<Chapter> {
        let Some(rows) = nyora_common::select_nonempty(doc, "#tab-chapter a") else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (i, a) in rows.iter().enumerate() {
            let Some(href) = a
                .attr("onclick")
                .and_then(|o| between(&o, "popupLogin('", "'"))
                .or_else(|| a.attr("href").filter(|h| !h.starts_with("javascript")))
            else {
                continue;
            };
            let key = self.strip_lang(&href);
            let date_uploaded = a
                .select_first(".cell-time")
                .and_then(|e| e.text())
                .and_then(|d| self.parse_date(d.trim()));
            out.push(Chapter {
                key: key.clone(),
                title: a.select_first(".cell-num").and_then(|e| e.text())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty()),
                chapter_number: Some((i + 1) as f32),
                date_uploaded,
                url: Some(self.abs(&key)),
                ..Default::default()
            });
        }
        out.reverse();
        out
    }

    /// `time[datetime]` may carry an ISO timestamp OR the same display format
    /// as the text nodes; the reference implementation feeds both through one
    /// pattern, so try that first and fall back to ISO.
    fn parse_date(&self, raw: &str) -> Option<i64> {
        date::parse(raw, self.cfg.date_pattern, self.cfg.locale)
            .or_else(|| date::parse(raw, "yyyy-MM-dd", self.cfg.locale))
    }

    // ---- pages -------------------------------------------------------------

    pub fn pages(&self, doc: &Document) -> Result<Vec<Page>> {
        let mut out = Vec::new();
        if let Some(imgs) = nyora_common::select_nonempty(doc, self.cfg.pages_sel()) {
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
        let doc = self.get(&format!("{}{}", self.cfg.base_url(), self.cfg.mangas_url))?;
        let mut out = Vec::new();
        if let Some(items) = nyora_common::select_nonempty(&doc, self.cfg.tags_sel()) {
            for a in items {
                let Some(href) = a.attr("href") else { continue };
                let key = href.trim_end_matches('/').rsplit('/').next().unwrap_or("").to_string();
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

fn between(s: &str, open: &str, close: &str) -> Option<String> {
    let i = s.find(open)? + open.len();
    let j = s[i..].find(close)?;
    Some(s[i..i + j].to_string())
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
