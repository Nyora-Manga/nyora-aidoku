//! MangaFire template for Aidoku.
//!
//! The only JSON-API source in the set — everything else scrapes HTML. Ported
//! from nyora-shared's MangaFireExtensionService.kt.
//!
//! The reference implementation routes through a residential proxy because
//! MangaFire blocks datacenter IPs at the Cloudflare edge. That constraint is
//! server-side only: an Aidoku extension runs on the user's device, so it hits
//! the API directly with no proxy. (It does mean this can't be tested from CI.)

#![no_std]
extern crate alloc;

pub mod config;

pub use config::MangaFireConfig;

use aidoku::{
    alloc::{string::String, string::ToString, vec::Vec},
    imports::net::{HttpMethod, Request},
    prelude::*,
    Chapter, ContentRating, Manga, MangaPageResult, MangaStatus, Page, PageContent, Result, Viewer,
};
use serde::Deserialize;

// ---- API shapes ------------------------------------------------------------

#[derive(Deserialize)]
struct ListResponse {
    #[serde(default)]
    items: Vec<Title>,
    #[serde(default)]
    meta: Meta,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Meta {
    #[serde(default)]
    has_next: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Title {
    hid: String,
    title: String,
    #[serde(default)]
    poster: Option<Poster>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
}

#[derive(Deserialize)]
struct Poster {
    #[serde(default)]
    large: Option<String>,
    #[serde(default)]
    medium: Option<String>,
    #[serde(default)]
    small: Option<String>,
}

impl Poster {
    fn best(&self) -> Option<String> {
        self.large.clone().or_else(|| self.medium.clone()).or_else(|| self.small.clone())
    }
}

#[derive(Deserialize)]
struct DetailsResponse {
    data: Details,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Details {
    #[serde(default)]
    title: String,
    #[serde(default)]
    poster: Option<Poster>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    synopsis_html: Option<String>,
    #[serde(default)]
    alt_titles: Vec<String>,
    #[serde(default)]
    genres: Vec<Named>,
    #[serde(default)]
    themes: Vec<Named>,
    #[serde(default)]
    authors: Vec<Named>,
    #[serde(default)]
    artists: Vec<Named>,
    #[serde(default)]
    content_rating: Option<String>,
}

#[derive(Deserialize)]
struct Named {
    #[serde(default)]
    title: String,
}

#[derive(Deserialize)]
struct ChaptersResponse {
    #[serde(default)]
    items: Vec<ChapterItem>,
    #[serde(default)]
    meta: ChapterMeta,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ChapterMeta {
    #[serde(default)]
    last_page: i32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChapterItem {
    id: i64,
    #[serde(default)]
    number: f32,
    #[serde(default)]
    name: String,
    #[serde(default)]
    r#type: Option<String>,
    /// Unix seconds — Aidoku wants seconds too, so no conversion.
    #[serde(default)]
    created_at: Option<i64>,
}

#[derive(Deserialize)]
struct PagesResponse {
    data: PagesData,
}

#[derive(Deserialize)]
struct PagesData {
    #[serde(default)]
    pages: Vec<PageItem>,
}

#[derive(Deserialize)]
struct PageItem {
    url: String,
}

pub struct MangaFireSource {
    pub cfg: &'static MangaFireConfig,
}

impl MangaFireSource {
    pub const fn new(cfg: &'static MangaFireConfig) -> Self {
        Self { cfg }
    }

    fn api(&self, path: &str) -> String {
        format!("https://{}/api{}", self.cfg.domain, path)
    }

    fn get<T: serde::de::DeserializeOwned>(&self, url: &str) -> Result<T> {
        Ok(Request::new(url, HttpMethod::Get)?.json_owned()?)
    }

    // ---- listing -----------------------------------------------------------

    /// `page` is 1-based on the wire, which is what Aidoku hands us, so it goes
    /// through unchanged.
    ///
    /// The API validates `order[...]` server-side and silently drops unknown
    /// filter params, so only the two verified sort keys are offered — inventing
    /// others returns an unfiltered list that merely looks filtered.
    pub fn search_filtered(
        &self,
        query: Option<String>,
        page: i32,
        order: Option<&str>,
        _genre: Option<&str>,
    ) -> Result<MangaPageResult> {
        let p = page.max(1);
        let url = match query.as_deref().filter(|q| !q.is_empty()) {
            Some(q) => self.api(&format!("/titles?keyword={}&page={}&limit=50", encode(q), p)),
            None => {
                let key = match order {
                    Some("latest") => "chapter_updated_at",
                    _ => "views_30d",
                };
                self.api(&format!("/titles?order%5B{key}%5D=desc&page={p}&limit=50"))
            }
        };
        let res: ListResponse = self.get(&url)?;
        let entries = res.items.iter().map(|t| self.to_manga(t)).collect::<Vec<_>>();
        Ok(MangaPageResult {
            has_next_page: res.meta.has_next && !entries.is_empty(),
            entries,
        })
    }

    pub fn search(&self, query: Option<String>, page: i32, order: Option<&str>) -> Result<MangaPageResult> {
        self.search_filtered(query, page, order, None)
    }

    pub fn listing(&self, id: &str, page: i32) -> Result<MangaPageResult> {
        let order = if id == "latest" { "latest" } else { "popular" };
        self.search_filtered(None, page, Some(order), None)
    }

    /// The manga key is the `hid`, not a URL — every API call is keyed by it.
    fn to_manga(&self, t: &Title) -> Manga {
        Manga {
            key: t.hid.clone(),
            title: t.title.clone(),
            cover: t.poster.as_ref().and_then(|p| p.best()),
            status: status_of(t.status.as_deref()),
            url: t.url.as_ref().map(|u| format!("https://{}{}", self.cfg.domain, u)),
            viewer: viewer_of(t.r#type.as_deref()),
            content_rating: if self.cfg.nsfw { ContentRating::NSFW } else { ContentRating::Safe },
            ..Default::default()
        }
    }

    // ---- details -----------------------------------------------------------

    pub fn details(&self, manga: &mut Manga) -> Result<()> {
        let res: DetailsResponse = self.get(&self.api(&format!("/titles/{}", manga.key)))?;
        let d = res.data;
        if !d.title.is_empty() {
            manga.title = d.title;
        }
        if let Some(c) = d.poster.as_ref().and_then(|p| p.best()) {
            manga.cover = Some(c);
        }
        manga.status = status_of(d.status.as_deref());
        manga.url = d.url.map(|u| format!("https://{}{}", self.cfg.domain, u));

        // genres and themes are separate lists but both read as tags
        let tags: Vec<String> = d
            .genres
            .iter()
            .chain(d.themes.iter())
            .map(|g| g.title.clone())
            .filter(|t| !t.is_empty())
            .collect();
        if !tags.is_empty() {
            manga.tags = Some(tags);
        }
        let authors: Vec<String> = d.authors.iter().map(|a| a.title.clone()).filter(|t| !t.is_empty()).collect();
        if !authors.is_empty() {
            manga.authors = Some(authors);
        }
        let artists: Vec<String> = d.artists.iter().map(|a| a.title.clone()).filter(|t| !t.is_empty()).collect();
        if !artists.is_empty() {
            manga.artists = Some(artists);
        }

        // The synopsis is an HTML fragment, so tags and entities are stripped.
        let syn = d.synopsis_html.as_deref().map(strip_html).filter(|s| !s.is_empty());
        let alts: Vec<String> = d.alt_titles.into_iter().filter(|s| !s.is_empty()).collect();
        manga.description = match (alts.is_empty(), syn) {
            (false, Some(s)) => Some(format!("Alternative: {}\n\n{}", alts.join(", "), s)),
            (false, None) => Some(format!("Alternative: {}", alts.join(", "))),
            (true, s) => s,
        };
        if matches!(d.content_rating.as_deref(), Some("erotica") | Some("pornographic")) {
            manga.content_rating = ContentRating::NSFW;
        }
        Ok(())
    }

    // ---- chapters ----------------------------------------------------------

    /// Paginated at 200/request; a long series is a handful of round trips.
    /// The API returns newest-first, so the list is reversed for numbering and
    /// handed back newest-first again.
    pub fn chapters(&self, key: &str) -> Result<Vec<Chapter>> {
        let cfg_lang = self.cfg.language;
        let mut all: Vec<ChapterItem> = Vec::new();
        let mut page = 1;
        loop {
            let url = self.api(&format!(
                "/titles/{}/chapters?language={}&sort=number&order=desc&page={}&limit=200",
                key, self.cfg.language, page
            ));
            let res: ChaptersResponse = self.get(&url)?;
            let last = res.meta.last_page;
            let empty = res.items.is_empty();
            all.extend(res.items);
            if empty || page >= last || page >= 20 {
                break;
            }
            page += 1;
        }

        Ok(all
            .into_iter()
            .map(|c| Chapter {
                // The numeric id IS the key: /api/chapters/{id} serves the pages
                // directly, so there's no URL to parse apart later.
                key: c.id.to_string(),
                title: Some(c.name).filter(|s| !s.trim().is_empty()),
                chapter_number: Some(c.number),
                date_uploaded: c.created_at.filter(|t| *t > 0),
                // "official" is the default and not worth surfacing; only
                // fan groups are shown as scanlators.
                scanlators: c
                    .r#type
                    .filter(|t| t != "official")
                    .map(|t| alloc::vec![t]),
                language: Some(cfg_lang.into()),
                url: None,
                ..Default::default()
            })
            .collect())
    }

    // ---- pages -------------------------------------------------------------

    pub fn pages(&self, chapter_key: &str) -> Result<Vec<Page>> {
        let res: PagesResponse = self.get(&self.api(&format!("/chapters/{chapter_key}")))?;
        Ok(res
            .data
            .pages
            .into_iter()
            .filter(|p| !p.url.is_empty())
            .map(|p| Page {
                content: PageContent::Url(p.url, None),
                ..Default::default()
            })
            .collect())
    }
}

fn status_of(s: Option<&str>) -> MangaStatus {
    match s.unwrap_or("") {
        "releasing" => MangaStatus::Ongoing,
        "finished" => MangaStatus::Completed,
        "on_hiatus" => MangaStatus::Hiatus,
        "discontinued" => MangaStatus::Cancelled,
        _ => MangaStatus::Unknown,
    }
}

fn viewer_of(t: Option<&str>) -> Viewer {
    match t.unwrap_or("") {
        "manhwa" | "manhua" => Viewer::Webtoon,
        _ => Viewer::RightToLeft,
    }
}

/// Strip tags and decode the entities the synopsis carries.
fn strip_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.replace("&#039;", "'")
        .replace("&quot;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
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
