//! Per-source configuration for the HotComics / TooMics template.
//!
//! Shape in repo/hotcomics.json is NESTED (`selectors:{}`), same as
//! mangareader.json.
//!
//! The distinguishing feature of this family is `langPath`: nine of the rows
//! are the SAME host (toomics.com) differing only by a language segment in the
//! path, so the effective base URL is host + langPath and the two must never be
//! treated as interchangeable.

extern crate alloc;
use alloc::{format, string::String};

pub struct HotComicsConfig {
    pub domain: &'static str,
    pub nsfw: bool,
    pub locale: &'static str,
    pub date_pattern: &'static str,

    /// Language segment appended to the host, e.g. "/en". Part of every URL.
    pub lang_path: &'static str,
    /// Browse path. Default "/genres"; the TooMics rows use a ranking path.
    pub mangas_url: &'static str,
    /// Site paginates in-page: anything past page 1 is empty.
    pub one_page: bool,
    pub search_supported: bool,
    /// Chapter links go through a JS login popup instead of plain hrefs.
    pub popup_login_chapters: bool,

    pub sel_mangas: Option<&'static str>,
    pub sel_chapters: Option<&'static str>,
    pub sel_tags_list: Option<&'static str>,
    pub sel_pages: Option<&'static str>,
}

pub const DEF_MANGAS: &str = "li[itemtype*=ComicSeries]:not(.no-comic)";
pub const DEF_CHAPTERS: &str = "#tab-chapter li";
pub const DEF_TAGS: &str = ".genres-list li:not(.on) a";
pub const DEF_PAGES: &str = "#viewer-img img";

impl HotComicsConfig {
    pub fn mangas_sel(&self) -> &str {
        self.sel_mangas.unwrap_or(DEF_MANGAS)
    }
    pub fn chapters_sel(&self) -> &str {
        self.sel_chapters.unwrap_or(DEF_CHAPTERS)
    }
    pub fn tags_sel(&self) -> &str {
        self.sel_tags_list.unwrap_or(DEF_TAGS)
    }
    pub fn pages_sel(&self) -> &str {
        self.sel_pages.unwrap_or(DEF_PAGES)
    }

    /// Host + language segment. Every URL builder goes through this.
    pub fn base_url(&self) -> String {
        format!("https://{}{}", self.domain, self.lang_path)
    }
}
