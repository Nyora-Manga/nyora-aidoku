//! Per-source configuration for MangaFire.
//!
//! One catalogue, several language editions: the sources differ ONLY by the
//! `language` query parameter on the chapter list.

pub struct MangaFireConfig {
    pub domain: &'static str,
    pub nsfw: bool,
    /// API language code for the chapter list (en, es, es-la, fr, ja, pt-br…).
    pub language: &'static str,
}
