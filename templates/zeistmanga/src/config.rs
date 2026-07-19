//! Per-source configuration for the ZeistManga template.
//!
//! Shape in repo/zeistmanga.json is FLAT scalars plus one nested `tagScrape`
//! object, which the generator flattens into the `tag_*` fields here.
//!
//! Unlike madara/mangareader there is no date-pattern normalisation: every row
//! uses the default `yyyy-MM-dd`, and the value is really an ISO timestamp we
//! truncate at the `T`, so no ICU quirks apply.

pub struct ZeistConfig {
    pub domain: &'static str,
    pub nsfw: bool,
    pub locale: &'static str,

    /// Blogger label that identifies a series post. Also the default browse label.
    pub manga_category: &'static str,
    pub max_results: i32,

    // Status vocabulary — these sites are localised, so the words that mean
    // "ongoing"/"completed" are per-source data rather than a fixed list.
    pub state_ongoing: &'static str,
    pub state_finished: &'static str,
    pub state_abandoned: &'static str,

    pub select_page: Option<&'static str>,
    pub select_tags: Option<&'static str>,

    /// Genres, when the theme exposes them as a scrapeable list.
    pub tag_path: Option<&'static str>,
    pub tag_root_id: Option<&'static str>,
    pub tag_root_sel: Option<&'static str>,
    pub tag_item: Option<&'static str>,
    pub tag_key_mode: Option<&'static str>,
    pub tag_title_mode: Option<&'static str>,

    /// Some themes ship a fixed genre list instead of a page to scrape.
    pub static_tags: &'static [(&'static str, &'static str)],
}

pub const DEF_PAGE: &str = "div.check-box img, article#reader .separator img, \
article.container .separator img, #readarea img, #reader img, #readerarea img";
pub const DEF_TAGS: &str = "article div.mt-15 a, .info-genre a";

impl ZeistConfig {
    pub fn page_sel(&self) -> &str {
        self.select_page.unwrap_or(DEF_PAGE)
    }
    pub fn tags_sel(&self) -> &str {
        self.select_tags.unwrap_or(DEF_TAGS)
    }
    pub fn base_url(&self) -> alloc::string::String {
        alloc::format!("https://{}", self.domain)
    }
}

extern crate alloc;
