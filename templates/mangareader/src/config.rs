//! Per-source configuration for the MangaReader / MangaThemesia template.
//!
//! Unlike madara.json, mangareader.json uses a NESTED shape — selectors live
//! under `config.selectors.{mangaList,…}`. The generator flattens that; this
//! struct is flat regardless.
//!
//! The engine also declares a `rawConfig` block (ListPageConfig / ChapterConfig
//! / PagesConfig / ApiConfig, ~180 lines of Kotlin) that ZERO of the 259 rows
//! set, so none of it is modelled here — same call as Madara's unset "absorb"
//! keys.
//!
//! Defaults come from MangaReaderEngine.kt:93-100.

extern crate alloc;
use alloc::{format, string::String};

pub struct MangaReaderConfig {
    pub domain: &'static str,
    pub nsfw: bool,
    pub locale: &'static str,
    pub date_pattern: &'static str,
    /// Browse path. `""` is a MEANINGFUL value (one row sets it) — it means the
    /// site root, not "unset", so this is not an Option.
    pub list_url: &'static str,
    /// Reader blob is base64 in a `data:text/javascript` script src (3 rows).
    pub encoded_src: bool,

    // --- selector overrides (23 of 259 rows set any of these) ---
    pub sel_manga_list: Option<&'static str>,
    pub sel_manga_list_img: Option<&'static str>,
    pub sel_manga_list_title: Option<&'static str>,
    pub sel_chapter: Option<&'static str>,
    pub sel_description: Option<&'static str>,
    pub sel_page: Option<&'static str>,
}

pub const DEF_MANGA_LIST: &str = ".postbody .listupd .bs .bsx";
pub const DEF_MANGA_LIST_IMG: &str = "img.ts-post-image";
pub const DEF_MANGA_LIST_TITLE: &str = "div.tt";
pub const DEF_CHAPTER: &str = "#chapterlist > ul > li";
pub const DEF_DESCRIPTION: &str = "div.entry-content";
pub const DEF_PAGE: &str = "div#readerarea img";

impl MangaReaderConfig {
    pub fn manga_list_sel(&self) -> &str {
        self.sel_manga_list.unwrap_or(DEF_MANGA_LIST)
    }
    pub fn manga_list_img_sel(&self) -> &str {
        self.sel_manga_list_img.unwrap_or(DEF_MANGA_LIST_IMG)
    }
    pub fn manga_list_title_sel(&self) -> &str {
        self.sel_manga_list_title.unwrap_or(DEF_MANGA_LIST_TITLE)
    }
    pub fn chapter_sel(&self) -> &str {
        self.sel_chapter.unwrap_or(DEF_CHAPTER)
    }
    pub fn description_sel(&self) -> &str {
        self.sel_description.unwrap_or(DEF_DESCRIPTION)
    }
    pub fn page_sel(&self) -> &str {
        self.sel_page.unwrap_or(DEF_PAGE)
    }

    pub fn base_url(&self) -> String {
        format!("https://{}", self.domain)
    }

    /// One row ships `dd-MM-yyy` — three `y`, which ICU does not read the same
    /// way Java's SimpleDateFormat does. Normalise year runs to 4 (or 2).
    pub fn normalized_date_pattern(&self) -> String {
        let mut out = String::with_capacity(self.date_pattern.len());
        let mut run = 0usize;
        let flush = |out: &mut String, run: usize| {
            if run > 0 {
                out.push_str(&"y".repeat(if run == 2 { 2 } else { 4 }));
            }
        };
        for ch in self.date_pattern.chars() {
            if ch == 'y' {
                run += 1;
                continue;
            }
            flush(&mut out, run);
            run = 0;
            out.push(ch);
        }
        flush(&mut out, run);
        out
    }
}
