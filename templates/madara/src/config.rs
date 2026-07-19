//! Per-source configuration for the Madara template.
//!
//! This mirrors the FLAT shape actually present in nyora-data-driven/repo/madara.json
//! (`selectPage`, `datePattern`, …), not the nested shape declared in
//! schema/SourceDef.schema.json. The schema also defines a dozen "absorb" keys
//! (chapterFetch, pathBrowse, listItem, tagDiscovery, staticTags, …) that ZERO
//! of the 551 rows actually set, so they are deliberately not modelled here.
//!
//! Defaults come from MadaraEngine.kt and must stay in sync with it.

pub struct MadaraConfig {
    pub domain: &'static str,
    pub nsfw: bool,
    /// BCP-47, for date parsing. Kotlin stores `Locale.ENGLISH`-style expressions
    /// in configComplex.sourceLocale; the generator maps those to "en" etc.
    pub locale: &'static str,

    // --- scalars (the five that carry almost all the variation) ---
    pub date_pattern: &'static str,
    pub tag_prefix: &'static str,
    pub list_url: &'static str,
    pub post_req: bool,
    pub without_ajax: bool,
    pub post_data_req: &'static str,
    /// NB: "" is a meaningful value here (14 rows), not "unset".
    pub style_page: &'static str,
    pub author_search_supported: bool,

    // --- selector overrides (only 36 of 551 rows set any of these) ---
    pub select_chapter: Option<&'static str>,
    pub select_page: Option<&'static str>,
    pub select_body_page: Option<&'static str>,
    pub select_desc: Option<&'static str>,
    pub select_genre: Option<&'static str>,
    pub select_date: Option<&'static str>,
    pub select_state: Option<&'static str>,
    pub select_alt: Option<&'static str>,
    pub select_test_async: Option<&'static str>,
}

// Stock selectors, from MadaraEngine.kt:1064-1085.
pub const DEF_CHAPTER: &str = "li.wp-manga-chapter, div.wp-manga-chapter";
pub const DEF_PAGE: &str = "div.page-break";
pub const DEF_BODY_PAGE: &str = "div.main-col-inner div.reading-content";
pub const DEF_GENRE: &str = "div.genres-content a";
pub const DEF_DATE: &str = "span.chapter-release-date i";
pub const DEF_TEST_ASYNC: &str = "div.listing-chapters_wrap";
pub const DEF_DESC: &str = "div.description-summary div.summary__content, \
div.summary_content div.post-content_item > h5 + div, \
div.summary_content div.manga-excerpt, div.post-content div.manga-summary, \
div.post-content div.desc, div.c-page__content div.summary__content";


impl MadaraConfig {
    pub fn chapter_sel(&self) -> &str {
        self.select_chapter.unwrap_or(DEF_CHAPTER)
    }
    pub fn page_sel(&self) -> &str {
        self.select_page.unwrap_or(DEF_PAGE)
    }
    pub fn body_page_sel(&self) -> &str {
        self.select_body_page.unwrap_or(DEF_BODY_PAGE)
    }
    pub fn desc_sel(&self) -> &str {
        self.select_desc.unwrap_or(DEF_DESC)
    }
    pub fn genre_sel(&self) -> &str {
        self.select_genre.unwrap_or(DEF_GENRE)
    }
    pub fn date_sel(&self) -> &str {
        self.select_date.unwrap_or(DEF_DATE)
    }
    pub fn test_async_sel(&self) -> &str {
        self.select_test_async.unwrap_or(DEF_TEST_ASYNC)
    }

    /// Java's SimpleDateFormat treats any run of >=4 `M` as the full month name,
    /// but Foundation/ICU (which backs Aidoku's read_date_string) reads `MMMMM`
    /// as the NARROW form — "J" instead of "January". 22 Spanish/Portuguese rows
    /// ship `dd 'de' MMMMM 'de' yyyy`, and every one of them would fail to parse
    /// if passed through unchanged.
    pub fn normalized_date_pattern(&self) -> alloc::string::String {
        use alloc::string::String;
        let mut out = String::with_capacity(self.date_pattern.len());
        let mut run = 0usize;
        for ch in self.date_pattern.chars() {
            if ch == 'M' {
                run += 1;
                continue;
            }
            if run > 0 {
                out.push_str(&"M".repeat(run.min(4)));
                run = 0;
            }
            out.push(ch);
        }
        if run > 0 {
            out.push_str(&"M".repeat(run.min(4)));
        }
        out
    }

    pub fn base_url(&self) -> alloc::string::String {
        alloc::format!("https://{}", self.domain)
    }
}

extern crate alloc;
