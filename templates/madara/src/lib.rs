//! Madara theme template for Aidoku, ported from nyora-data-driven's
//! MadaraEngine.kt. One WASM module per source; all per-site variation comes
//! from the `MadaraConfig` the generated crate supplies.
//!
//! Covers 553 of the 1346 kotatsu sources (the single largest family). Parsing
//! happens entirely on-device: the only network traffic is to the source site,
//! and Cloudflare interstitials are solved host-side by Aidoku's WebView
//! handler, so no helper server is involved at any point.

#![no_std]
extern crate alloc;

pub mod config;

pub use config::MadaraConfig;
pub use nyora_common::{date, IMG_ATTRS};

use aidoku::{
    alloc::{string::String, string::ToString, vec::Vec},
    imports::{
        html::{Document, Element},
        net::{Request, HttpMethod},
    },
    prelude::*,
    Chapter, ContentRating, Manga, MangaPageResult, MangaStatus, Page, PageContent, Result,
    Viewer,
};

pub struct MadaraSource {
    pub cfg: &'static MadaraConfig,
}

impl MadaraSource {
    pub const fn new(cfg: &'static MadaraConfig) -> Self {
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

    /// Manga key = site-relative path, so keys stay stable across domain moves.
    fn rel(&self, url: &str) -> String {
        let base = self.cfg.base_url();
        let stripped = url.strip_prefix(&base).unwrap_or(url);
        let stripped = stripped.strip_prefix("https://").unwrap_or(stripped);
        if stripped.starts_with('/') {
            stripped.into()
        } else if stripped.contains('/') && !url.starts_with('/') && url.starts_with("http") {
            // absolute URL on another host: keep the path only
            let after = stripped.split_once('/').map(|x| x.1).unwrap_or(stripped);
            format!("/{after}")
        } else {
            format!("/{stripped}")
        }
    }

    /// Pick the first usable image URL. `src` is checked LAST because Madara
    /// themes lazy-load — `src` is usually a 1px placeholder while the real URL
    /// sits in a data-* attribute. Inline `data:` URIs are skipped for the same
    /// reason.
    fn img_src(&self, el: &Element) -> Option<String> {
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
                    return Some(self.abs(first));
                }
            }
        }
        None
    }

    // ---- fetch helpers -----------------------------------------------------

    pub fn fetch_manga(&self, key: &str) -> Result<Document> {
        Ok(Request::new(&self.abs(key), HttpMethod::Get)?.html()?)
    }

    pub fn fetch_chapter(&self, key: &str) -> Result<Document> {
        Ok(Request::new(&self.abs(key), HttpMethod::Get)?.html()?)
    }

    // ---- listing -----------------------------------------------------------

    /// Two mutually exclusive list transports, per `withoutAjax`:
    ///   false (498/551 rows) → POST admin-ajax `madara_load_more`
    ///   true                 → GET `?s=&post_type=wp-manga`
    /// Note the page-index difference: admin-ajax is 0-based, the GET form is
    /// 1-based. Mixing them up silently skips or repeats a page.
    pub fn search(
        &self,
        query: Option<String>,
        page: i32,
        listing_order: Option<&str>,
    ) -> Result<MangaPageResult> {
        self.search_filtered(query, page, listing_order, None)
    }

    /// `genre` is a tag slug (the part after `tagPrefix`). Madara serves a
    /// genre as its own archive path rather than a query parameter, so it
    /// bypasses both search transports entirely.
    pub fn search_filtered(
        &self,
        query: Option<String>,
        page: i32,
        listing_order: Option<&str>,
        genre: Option<&str>,
    ) -> Result<MangaPageResult> {
        if let Some(g) = genre {
            let mut url = format!(
                "{}/{}{}/page/{}/",
                self.cfg.base_url(),
                self.cfg.tag_prefix,
                g,
                page.max(1)
            );
            if let Some(order) = listing_order {
                url.push_str(&format!("?m_orderby={order}"));
            }
            let doc = Request::new(&url, HttpMethod::Get)?.html()?;
            let entries = self.parse_manga_list(&doc);
            return Ok(MangaPageResult {
                has_next_page: !entries.is_empty(),
                entries,
            });
        }
        let doc = if self.cfg.without_ajax {
            let mut url = format!(
                "{}/page/{}/?s={}&post_type=wp-manga",
                self.cfg.base_url(),
                page.max(1),
                query.as_deref().unwrap_or("")
            );
            if let Some(order) = listing_order {
                url.push_str(&format!("&m_orderby={order}"));
            }
            Request::new(&url, HttpMethod::Get)?.html()?
        } else {
            let body = self.ajax_body(query.as_deref(), page, listing_order);
            Request::new(
                &format!("{}/wp-admin/admin-ajax.php", self.cfg.base_url()),
                HttpMethod::Post,
            )?
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Referer", &self.cfg.base_url())
            .body(body)
            .html()?
        };
        let entries = self.parse_manga_list(&doc);
        Ok(MangaPageResult {
            has_next_page: !entries.is_empty(),
            entries,
        })
    }

    /// The admin-ajax payload. Order matters — WordPress reads these as an
    /// ordered `vars[...]` map — so this builds a Vec, never a HashMap.
    fn ajax_body(&self, query: Option<&str>, page: i32, order: Option<&str>) -> String {
        let mut p: Vec<(String, String)> = Vec::new();
        let mut push = |k: &str, v: &str| p.push((k.into(), v.into()));
        push("action", "madara_load_more");
        push("page", &(page.max(1) - 1).to_string()); // 0-based here
        push("template", "madara-core/content/content-search");
        push("vars[s]", query.unwrap_or(""));
        push("vars[paged]", "1");
        push("vars[template]", "search");
        push("vars[post_type]", "wp-manga");
        push("vars[post_status]", "publish");
        push("vars[posts_per_page]", "20");
        match order.unwrap_or("views") {
            "latest" => {
                push("vars[orderby]", "meta_value_num");
                push("vars[meta_key]", "_latest_update");
                push("vars[order]", "desc");
            }
            "alphabet" => {
                push("vars[orderby]", "post_title");
                push("vars[order]", "asc");
            }
            "rating" => {
                push("vars[orderby][query_average_reviews]", "desc");
                push("vars[orderby][query_total_reviews]", "desc");
            }
            "new-manga" => {
                push("vars[orderby]", "date");
                push("vars[order]", "desc");
            }
            _ => {
                push("vars[orderby]", "meta_value_num");
                push("vars[meta_key]", "_wp_manga_views");
                push("vars[order]", "desc");
            }
        }
        p.iter()
            .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
            .collect::<Vec<_>>()
            .join("&")
    }

    /// The stock Madara card grid. Both the ajax fragment and the full search
    /// page render the same markup, so one parser serves both.
    pub fn parse_manga_list(&self, doc: &Document) -> Vec<Manga> {
        let mut out: Vec<Manga> = Vec::new();
        let Some(items) = doc.select("div.row.c-tabs-item__content, div.page-item-detail") else {
            return out;
        };
        for item in items {
            let Some(link) = item
                .select_first("h3 a, h4 a, .post-title a, .manga-name a")
                .or_else(|| item.select_first("a"))
            else {
                continue;
            };
            let Some(href) = link.attr("href") else { continue };
            let title = link.text().unwrap_or_default().trim().to_string();
            if title.is_empty() {
                continue;
            }
            let cover = item.select_first("img").and_then(|i| self.img_src(&i));
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

    // ---- listings & tags ---------------------------------------------------

    /// The listing ids declared in every generated res/source.json. These map
    /// onto Madara's own m_orderby values.
    pub fn listing(&self, id: &str, page: i32) -> Result<MangaPageResult> {
        let order = match id {
            "latest" => "latest",
            "new" => "new-manga",
            "alphabetical" => "alphabet",
            "rating" => "rating",
            _ => "views",
        };
        self.search_filtered(None, page, Some(order), None)
    }

    /// Genres come from the site's own menus. kotatsu unions two independent
    /// roots because themes put the list in one or the other; a source that
    /// uses the second would otherwise report no genres at all.
    pub fn genres(&self) -> Result<Vec<(String, String)>> {
        let doc = Request::new(
            &format!("{}/{}", self.cfg.base_url(), self.cfg.list_url),
            HttpMethod::Get,
        )?
        .html()?;
        let mut out: Vec<(String, String)> = Vec::new();
        for sel in ["header ul.second-menu li a", "div.genres_wrap ul.list-unstyled li a"] {
            let Some(items) = doc.select(sel) else { continue };
            for a in items {
                let Some(href) = a.attr("href") else { continue };
                let trimmed = href.trim_end_matches('/');
                // 2-arg substringAfterLast semantics: NO match means skip the
                // row, not "use the whole href" — otherwise every unrelated
                // menu link becomes a bogus genre.
                let Some(slug) = trimmed.rsplit_once(self.cfg.tag_prefix.trim_end_matches('/'))
                    .map(|(_, t)| t.trim_matches('/'))
                    .filter(|t| !t.is_empty())
                else {
                    continue;
                };
                let name = a.own_text().or_else(|| a.text()).unwrap_or_default();
                let name = name.trim().to_string();
                if name.is_empty() || out.iter().any(|(_, s)| s == slug) {
                    continue;
                }
                out.push((name, slug.into()));
            }
        }
        Ok(out)
    }

    /// Alt titles sit in a value cell identified only by its sibling heading,
    /// same shape as status. kotatsu reaches it by walking `parent()` until it
    /// finds a node with exactly two children — the new Aidoku runner exposes
    /// `parent`/`children`, but iterating labelled rows is simpler and doesn't
    /// depend on the markup nesting exactly two deep.
    fn alt_titles(&self, doc: &Document) -> Vec<String> {
        const LABELS: &[&str] = &[
            "alternative", "alt title", "alternate", "otros nombres", "outros nomes",
            "autres noms", "alternatif", "另名", "الاسم الاخر",
        ];
        let mut out: Vec<String> = Vec::new();
        if let Some(sel) = self.cfg.select_alt {
            if let Some(t) = doc.select_first(sel).and_then(|e| e.text()) {
                push_alts(&mut out, &t);
            }
            return out;
        }
        if let Some(rows) = doc.select("div.post-content_item") {
            for row in rows {
                let heading = row
                    .select_first("div.summary-heading, h5")
                    .and_then(|h| h.text())
                    .unwrap_or_default()
                    .trim()
                    .to_lowercase();
                if heading.is_empty() || !LABELS.iter().any(|l| heading.contains(l)) {
                    continue;
                }
                if let Some(v) = row.select_first("div.summary-content").and_then(|c| c.text()) {
                    push_alts(&mut out, &v);
                    break;
                }
            }
        }
        out
    }

    // ---- details -----------------------------------------------------------

    pub fn details(&self, manga: &mut Manga, doc: &Document) {
        if let Some(t) = doc.select_first("h1").and_then(|e| e.text()) {
            let t = t.trim();
            if !t.is_empty() {
                manga.title = t.into();
            }
        }
        manga.cover = doc
            .select_first("div.summary_image img")
            .and_then(|i| self.img_src(&i))
            .or_else(|| manga.cover.clone());

        let authors: Vec<String> = doc
            .select("div.author-content a")
            .map(|l| l.filter_map(|e| e.text()).collect())
            .unwrap_or_default();
        if !authors.is_empty() {
            manga.authors = Some(authors);
        }
        let artists: Vec<String> = doc
            .select("div.artist-content a")
            .map(|l| l.filter_map(|e| e.text()).collect())
            .unwrap_or_default();
        if !artists.is_empty() {
            manga.artists = Some(artists);
        }

        manga.description = doc
            .select_first(self.cfg.desc_sel())
            .and_then(|e| e.text())
            .map(|s| s.trim().to_string());

        let tags: Vec<String> = doc
            .select(self.cfg.genre_sel())
            .map(|l| l.filter_map(|e| e.text()).collect())
            .unwrap_or_default();
        if !tags.is_empty() {
            manga.tags = Some(tags);
        }

        // Aidoku's Manga has no alternate-titles field, so surface them the way
        // other Aidoku sources do — as a line above the synopsis. Dropping them
        // would lose the romanised/native names people search by.
        let alts = self.alt_titles(doc);
        if !alts.is_empty() {
            let joined = alts.join(", ");
            manga.description = Some(match manga.description.take() {
                Some(d) if !d.is_empty() => format!("Alternative: {joined}\n\n{d}"),
                _ => format!("Alternative: {joined}"),
            });
        }
        manga.status = self.status(doc);
        manga.url = Some(self.abs(&manga.key));

        // A site marking a title adult overrides the source-level flag, matching
        // kotatsu: .adult-confirm is the interstitial Madara shows for 18+.
        if doc.select_first("div.adult-confirm, .adult_confirm").is_some() {
            manga.content_rating = ContentRating::NSFW;
        }
        manga.viewer = Viewer::Webtoon;
    }

    /// Status sits in a value cell whose only identifier is the label in its
    /// sibling heading. kotatsu selects it with a 13-way `:contains()`
    /// alternation, but `:contains()` is a Jsoup/SwiftSoup extension, not
    /// standard CSS — it isn't supported by every HTML engine (the Aidoku test
    /// runner rejects it outright). Iterating the rows and comparing heading
    /// text uses plain CSS only, works identically everywhere, and handles the
    /// non-ASCII labels (状态, حالة العمل, สถานะ) that would be fragile inside a
    /// selector string.
    fn status(&self, doc: &Document) -> MangaStatus {
        const LABELS: &[&str] = &[
            "status", "estado", "durum", "statut", "statut ", "状态", "الحالة", "حالة العمل",
            "สถานะ", "статус", "trạng thái", "stato", "zustand",
        ];
        let text = if let Some(sel) = self.cfg.select_state {
            doc.select_first(sel).and_then(|e| e.text())
        } else {
            let mut found: Option<String> = None;
            if let Some(rows) = doc.select("div.post-content_item, div.post-status div") {
                for row in rows {
                    let heading = row
                        .select_first("div.summary-heading, h5")
                        .and_then(|h| h.text())
                        .unwrap_or_default()
                        .trim()
                        .to_lowercase();
                    if heading.is_empty() {
                        continue;
                    }
                    if LABELS.iter().any(|l| heading.contains(l)) {
                        found = row.select_first("div.summary-content").and_then(|c| c.text());
                        if found.is_some() {
                            break;
                        }
                    }
                }
            }
            found
        };
        let Some(text) = text else {
            return MangaStatus::Unknown;
        };
        let t = text.trim().to_lowercase();
        if t.contains("complet") || t.contains("completo") || t.contains("finished") {
            MangaStatus::Completed
        } else if t.contains("ongoing")
            || t.contains("en cours")
            || t.contains("lançamento")
            || t.contains("devam")
            || t.contains("publishing")
        {
            MangaStatus::Ongoing
        } else if t.contains("cancel") || t.contains("drop") {
            MangaStatus::Cancelled
        } else if t.contains("hiatus") || t.contains("pausa") {
            MangaStatus::Hiatus
        } else {
            MangaStatus::Unknown
        }
    }

    // ---- chapters ----------------------------------------------------------

    /// Chapters are served three ways depending on the theme's age:
    ///   inline in the details page,
    ///   POST {manga}/ajax/chapters/,
    ///   POST admin-ajax action=manga_get_chapters (post_req).
    /// Try inline first, then whichever POST the config selects.
    pub fn chapters(&self, manga_key: &str, doc: &Document) -> Result<Vec<Chapter>> {
        let inline = self.map_chapters(doc);
        if !inline.is_empty() {
            return Ok(inline);
        }
        let url = if self.cfg.post_req {
            format!("{}/wp-admin/admin-ajax.php", self.cfg.base_url())
        } else {
            format!("{}{}/ajax/chapters/", self.cfg.base_url(), manga_key.trim_end_matches('/'))
        };
        let mut req = Request::new(&url, HttpMethod::Post)?
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Referer", &self.abs(manga_key));
        if self.cfg.post_req {
            // needs the numeric post id, exposed on the shortlink/body class
            let id = doc
                .select_first("div#manga-chapters-holder[data-id]")
                .and_then(|e| e.attr("data-id"))
                .unwrap_or_default();
            req = req.body(format!("{}{}", self.cfg.post_data_req, id));
        } else {
            req = req.body("");
        }
        let ajax = req.html()?;
        Ok(self.map_chapters(&ajax))
    }

    /// Numbering must count only KEPT rows, and dedupe as it goes — a duplicate
    /// href that still advanced the counter would leave gaps in the chapter
    /// numbers. Source order is newest-first, so iterate reversed.
    fn map_chapters(&self, doc: &Document) -> Vec<Chapter> {
        let Some(rows) = doc.select(self.cfg.chapter_sel()) else {
            return Vec::new();
        };
        let rows: Vec<Element> = rows.collect();
        let mut seen: Vec<String> = Vec::new();
        let mut out: Vec<Chapter> = Vec::new();
        let mut index = 0f32;
        for li in rows.iter().rev() {
            let Some(a) = li.select_first("a") else { continue };
            let Some(href) = a.attr("href") else { continue };
            let key = self.rel(&href);
            if seen.iter().any(|s| s == &key) {
                continue;
            }
            seen.push(key.clone());
            index += 1.0;

            // c-new-tag carries an exact timestamp in title=; the visible date
            // node is often relative ("2 hours ago"), so prefer the tag.
            let raw_date = li
                .select_first("a.c-new-tag")
                .and_then(|e| e.attr("title"))
                .or_else(|| li.select_first(self.cfg.date_sel()).and_then(|e| e.text()));
            let date_uploaded = raw_date.and_then(|d| {
                date::parse(d.trim(), &self.cfg.normalized_date_pattern(), self.cfg.locale)
            });

            let title = a
                .select_first("p")
                .and_then(|e| e.text())
                .or_else(|| a.own_text())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            out.push(Chapter {
                key: format!("{key}{}", self.cfg.style_page),
                title,
                chapter_number: Some(index),
                date_uploaded,
                url: Some(self.abs(&key)),
                ..Default::default()
            });
        }
        out.reverse(); // present newest-first
        out
    }

    // ---- pages -------------------------------------------------------------

    /// Pages are nested two levels: a container per page, each holding the img.
    /// Scope to the reading-content root when it exists so navigation/related
    /// thumbnails elsewhere on the page can't leak in as pages.
    pub fn pages(&self, doc: &Document) -> Result<Vec<Page>> {
        let containers = doc
            .select_first(self.cfg.body_page_sel())
            .and_then(|root| root.select(self.cfg.page_sel()))
            .or_else(|| doc.select(self.cfg.page_sel()));

        let mut out: Vec<Page> = Vec::new();
        if let Some(containers) = containers {
            for c in containers {
                if let Some(imgs) = c.select("img") {
                    for img in imgs {
                        if let Some(u) = self.img_src(&img) {
                            out.push(Page {
                                content: PageContent::Url(u, None),
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }
        Ok(out)
    }
}

/// Percent-encoding for form bodies (application/x-www-form-urlencoded).
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

/// Sites list alt titles in one cell separated by commas, semicolons or pipes.
fn push_alts(out: &mut Vec<String>, raw: &str) {
    for part in raw.split(['|', ';', ',']) {
        let t = part.trim();
        if !t.is_empty() && !out.iter().any(|e| e == t) {
            out.push(t.into());
        }
    }
}
