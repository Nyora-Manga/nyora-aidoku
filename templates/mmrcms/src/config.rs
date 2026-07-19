//! Per-source configuration for the MMRCMS template.
//!
//! Shape in repo/mmrcms.json is NESTED (`selectors:{}`) plus flat scalars.
//!
//! The `label_*` fields hold the LABEL TEXT only, not a selector: kotatsu ships
//! these as `dt:contains(Statut)`, but `:contains()` is a Jsoup extension the
//! Aidoku selector engine rejects, so the generator strips the wrapper and the
//! template matches the label in Rust.

extern crate alloc;
use alloc::{format, string::String};

pub struct MmrcmsConfig {
    pub domain: &'static str,
    pub nsfw: bool,
    pub locale: &'static str,
    pub date_pattern: &'static str,
    pub list_url: &'static str,
    pub tag_url: &'static str,
    /// Cover filename appended to /uploads/manga/{slug} on the latest grid,
    /// which ships no <img> of its own.
    pub img_updated: &'static str,

    pub label_state: &'static str,
    pub label_author: &'static str,
    pub label_tag: &'static str,
    pub label_alt: &'static str,
}

impl MmrcmsConfig {
    pub fn base_url(&self) -> String {
        format!("https://{}", self.domain)
    }
}
