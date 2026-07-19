//! Per-source configuration for the AsuraScans template.
//!
//! One row, all defaults — the config exists for symmetry with the other
//! templates and so the domain can be corrected without touching code.

extern crate alloc;
use alloc::{format, string::String};

pub struct AsuraConfig {
    pub domain: &'static str,
    pub nsfw: bool,
    pub locale: &'static str,
    /// Genres are rendered client-side on /browse, so they can't be scraped and
    /// are compiled in instead.
    pub genres: &'static [&'static str],
}

impl AsuraConfig {
    pub fn base_url(&self) -> String {
        format!("https://{}", self.domain)
    }
}
