#!/usr/bin/env python3
"""Generate Aidoku source crates from the vendored definitions in data/.

One crate per source; every crate is identical except for the CONFIG block, so
the parsing logic lives in exactly one place (templates/madara). This is what
makes 553 sources tractable: they are data, not code.

    python3 tools/generate.py                 # all non-broken madara sources
    python3 tools/generate.py --id MANGAREAD  # just one
    python3 tools/generate.py --limit 20      # first N, for a smoke run

Broken rows are skipped by default: nyora-data-driven marks 237 of the 551
Madara sources dead upstream, and shipping those would just be 237 sources that
always fail.
"""
import argparse
import json
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
DATA = ROOT / "data"

# Each engine: which JSON, which Rust template, and how to build its CONFIG
# literal. madara.json is FLAT (selectPage); mangareader.json is NESTED
# (selectors.mangaList) — the emitters below absorb that difference so the rest
# of the generator doesn't care.
ENGINES = ("madara", "mangareader", "zeistmanga", "hotcomics", "onemanga",
           "mmrcms", "asurascans", "mangafire")

# Measured HTTP status per source, from a live sweep (tools/liveness.py).
# Only consulted with --live-only. The default is to emit EVERY row so the
# generated catalogue matches the Android app's, matching how nyora-android and
# nyora-aidoku ship: a dead source is listed and simply fails, rather than
# silently missing from the list.
LIVENESS = ROOT / "data-liveness.json"

# Favicons fetched by tools/icons.py. Aidoku renders res/icon.png from the
# package; without one every source shows a placeholder. Cached here so they
# survive regeneration and are only fetched once.
ICONS = ROOT / "icons"

# kotatsu stores these as Kotlin expressions, not BCP-47 tags.
LOCALE_MAP = {
    "Locale.ENGLISH": "en", "Locale.US": "en", "Locale.UK": "en",
    "Locale.FRENCH": "fr", "Locale.FRANCE": "fr",
    "Locale.GERMAN": "de", "Locale.GERMANY": "de",
    "Locale.ITALIAN": "it", "Locale.ITALY": "it",
    "Locale.JAPANESE": "ja", "Locale.JAPAN": "ja",
    "Locale.KOREAN": "ko", "Locale.KOREA": "ko",
    "Locale.CHINESE": "zh", "Locale.SIMPLIFIED_CHINESE": "zh-Hans",
    "Locale.ROOT": "", "Locale.getDefault()": "",
}


def locale_for(row):
    raw = (row.get("configComplex") or {}).get("sourceLocale")
    if not raw:
        return row.get("lang") or "en"
    if raw in LOCALE_MAP:
        return LOCALE_MAP[raw]
    # Locale("ar") / Locale("th", "TH")
    m = re.search(r'Locale\(\s*"([a-zA-Z-]+)"', raw)
    return m.group(1) if m else (row.get("lang") or "en")


def rs_str(v):
    return '"' + str(v).replace("\\", "\\\\").replace('"', '\\"') + '"'


def rs_opt(v):
    return f"Some({rs_str(v)})" if v else "None"


def crate_name(source_id, lang):
    slug = re.sub(r"[^a-z0-9]+", "-", source_id.lower()).strip("-")
    return f"{lang}-{slug}"


def pkg_id(source_id, lang):
    slug = re.sub(r"[^a-z0-9]+", "", source_id.lower())
    return f"{lang}.{slug}"


def madara_config(cfg, row):
    return f"""    domain: {rs_str(row["domain"])},
    nsfw: {str(bool(row.get('nsfw'))).lower()},
    locale: {rs_str(locale_for(row))},
    date_pattern: {rs_str(cfg.get('datePattern', 'MMMM d, yyyy'))},
    tag_prefix: {rs_str(cfg.get('tagPrefix', 'manga-genre/'))},
    list_url: {rs_str(cfg.get('listUrl', 'manga/'))},
    post_req: {str(bool(cfg.get('postReq'))).lower()},
    without_ajax: {str(bool(cfg.get('withoutAjax'))).lower()},
    post_data_req: {rs_str(cfg.get('postDataReq', 'action=manga_get_chapters&manga='))},
    style_page: {rs_str(cfg.get('stylePage', '?style=list'))},
    author_search_supported: {str(bool(cfg.get('authorSearchSupported'))).lower()},
    select_chapter: {rs_opt(cfg.get('selectChapter'))},
    select_page: {rs_opt(cfg.get('selectPage'))},
    select_body_page: {rs_opt(cfg.get('selectBodyPage'))},
    select_desc: {rs_opt(cfg.get('selectDesc'))},
    select_genre: {rs_opt(cfg.get('selectGenre'))},
    select_date: {rs_opt(cfg.get('selectDate'))},
    select_state: {rs_opt(cfg.get('selectState'))},
    select_alt: {rs_opt(cfg.get('selectAlt'))},
    select_test_async: {rs_opt(cfg.get('selectTestAsync'))},"""


def mangareader_config(cfg, row):
    # nested block, unlike madara's flat keys
    sel = cfg.get("selectors") or {}
    # listUrl "" is meaningful (site root), so only default when ABSENT
    list_url = cfg["listUrl"] if "listUrl" in cfg else "/manga"
    return f"""    domain: {rs_str(row["domain"])},
    nsfw: {str(bool(row.get('nsfw'))).lower()},
    locale: {rs_str(cfg.get('locale') or row.get('lang') or 'en')},
    date_pattern: {rs_str(cfg.get('datePattern', 'MMM d, yyyy'))},
    list_url: {rs_str(list_url)},
    encoded_src: {str(bool(cfg.get('encodedSrc'))).lower()},
    sel_manga_list: {rs_opt(sel.get('mangaList'))},
    sel_manga_list_img: {rs_opt(sel.get('mangaListImg'))},
    sel_manga_list_title: {rs_opt(sel.get('mangaListTitle'))},
    sel_chapter: {rs_opt(sel.get('chapter'))},
    sel_description: {rs_opt(sel.get('description'))},
    sel_page: {rs_opt(sel.get('page'))},"""


def zeistmanga_config(cfg, row):
    ts = cfg.get("tagScrape") or {}
    static = cfg.get("staticTags") or []
    lits = ", ".join(
        f"({rs_str(t.get('title', ''))}, {rs_str(t.get('key', ''))})" for t in static
    )
    return f"""    domain: {rs_str(row["domain"])},
    nsfw: {str(bool(row.get('nsfw'))).lower()},
    locale: {rs_str(cfg.get('locale') or row.get('lang') or 'en')},
    manga_category: {rs_str(cfg.get('mangaCategory', 'Series'))},
    max_results: {int(cfg.get('maxMangaResults', 20))},
    state_ongoing: {rs_str(cfg.get('stateOngoing', 'Ongoing'))},
    state_finished: {rs_str(cfg.get('stateFinished', 'Completed'))},
    state_abandoned: {rs_str(cfg.get('stateAbandoned', 'Cancelled'))},
    select_page: {rs_opt(cfg.get('selectPage'))},
    select_tags: {rs_opt(cfg.get('selectTags'))},
    tag_path: {rs_opt(ts.get('path'))},
    tag_root_id: {rs_opt(ts.get('rootId'))},
    tag_root_sel: {rs_opt(ts.get('rootSelector'))},
    tag_item: {rs_opt(ts.get('item'))},
    tag_key_mode: {rs_opt(ts.get('keyMode'))},
    tag_title_mode: {rs_opt(ts.get('titleMode'))},
    static_tags: &[{lits}],"""


def hotcomics_config(cfg, row):
    sel = cfg.get("selectors") or {}
    return f"""    domain: {rs_str(row["domain"])},
    nsfw: {str(bool(row.get('nsfw'))).lower()},
    locale: {rs_str(cfg.get('locale') or row.get('lang') or 'en')},
    date_pattern: {rs_str(cfg.get('datePattern', 'MMM dd, yyyy'))},
    lang_path: {rs_str(cfg.get('langPath', ''))},
    mangas_url: {rs_str(cfg.get('mangasUrl', '/genres'))},
    one_page: {str(bool(cfg.get('onePage'))).lower()},
    search_supported: {str(cfg.get('searchSupported', True)).lower()},
    popup_login_chapters: {str(bool(cfg.get('popupLoginChapters'))).lower()},
    sel_mangas: {rs_opt(sel.get('mangas'))},
    sel_chapters: {rs_opt(sel.get('chapters'))},
    sel_tags_list: {rs_opt(sel.get('tagsList'))},
    sel_pages: {rs_opt(sel.get('pages'))},"""


def onemanga_config(cfg, row):
    # All 24 rows ship an empty config; only the domain varies.
    return f"""    domain: {rs_str(row["domain"])},
    nsfw: {str(bool(row.get('nsfw'))).lower()},
    locale: {rs_str(cfg.get('locale') or row.get('lang') or 'en')},"""


DEF_TAG_LABEL = "Cat\u00e9gories"  # "Catégories" — kept out of the f-string


def _label(v, default):
    """kotatsu ships `dt:contains(Statut)`; the template wants just `Statut`."""
    raw = v or default
    if ":contains(" in raw:
        raw = raw.split(":contains(", 1)[1].rsplit(")", 1)[0]
    return raw


def mmrcms_config(cfg, row):
    sel = cfg.get("selectors") or {}
    return f"""    domain: {rs_str(row["domain"])},
    nsfw: {str(bool(row.get('nsfw'))).lower()},
    locale: {rs_str(cfg.get('locale') or row.get('lang') or 'en')},
    date_pattern: {rs_str(cfg.get('datePattern', 'dd MMM. yyyy'))},
    list_url: {rs_str(cfg.get('listUrl', 'filterList'))},
    tag_url: {rs_str(cfg.get('tagUrl', 'manga-list'))},
    img_updated: {rs_str(cfg.get('imgUpdated', '/cover/cover_250x350.jpg'))},
    label_state: {rs_str(_label(sel.get('state'), 'Statut'))},
    label_author: {rs_str(_label(sel.get('author'), 'Auteur(s)'))},
    label_tag: {rs_str(_label(sel.get('tag'), DEF_TAG_LABEL))},
    label_alt: {rs_str(_label(sel.get('alt'), 'Autres noms'))},"""


ASURA_GENRES = (
    "Action", "Adventure", "Comedy", "Drama", "Fantasy", "Historical", "Horror",
    "Isekai", "Josei", "Manhua", "Manhwa", "Martial Arts", "Mature", "Mecha",
    "Mystery", "Psychological", "Romance", "School Life", "Sci-fi", "Seinen",
    "Shoujo", "Shounen", "Slice of Life", "Sports", "Supernatural", "Thriller",
    "Tragedy", "Crazy MC", "Genius MC", "Overpowered", "Reincarnation", "Revenge",
    "System", "Time Travel", "Villain",
)


def asurascans_config(cfg, row):
    genres = ", ".join(rs_str(g) for g in ASURA_GENRES)
    return f"""    domain: {rs_str(row["domain"])},
    nsfw: {str(bool(row.get('nsfw'))).lower()},
    locale: {rs_str(cfg.get('locale') or row.get('lang') or 'en')},
    genres: &[{genres}],"""


def mangafire_config(cfg, row):
    return f"""    domain: {rs_str(row["domain"])},
    nsfw: {str(bool(row.get('nsfw'))).lower()},
    language: {rs_str(cfg.get('language', 'en'))},"""


SPECS = {
    "madara": ("madara-template", "MadaraConfig", "MadaraSource", madara_config),
    "mangareader": ("mangareader-template", "MangaReaderConfig", "MangaReaderSource",
                    mangareader_config),
    "zeistmanga": ("zeistmanga-template", "ZeistConfig", "ZeistSource", zeistmanga_config),
    "hotcomics": ("hotcomics-template", "HotComicsConfig", "HotComicsSource", hotcomics_config),
    "onemanga": ("onemanga-template", "OneMangaConfig", "OneMangaSource", onemanga_config),
    "mmrcms": ("mmrcms-template", "MmrcmsConfig", "MmrcmsSource", mmrcms_config),
    "asurascans": ("asurascans-template", "AsuraConfig", "AsuraSource", asurascans_config),
    "mangafire": ("mangafire-template", "MangaFireConfig", "MangaFireSource", mangafire_config),
}

# zeistmanga browses a Blogger feed that only supports orderby=published, so it
# exposes status listings rather than sort orders.
LISTINGS = {
    # single-series sites: one listing, since popular/latest/new are identical
    "onemanga": [{"id": "series", "name": "Series"}],
    "asurascans": [
        {"id": "latest", "name": "Latest"},
        {"id": "popular", "name": "Popular"},
        {"id": "rating", "name": "Top Rated"},
    ],
    "zeistmanga": [
        {"id": "series", "name": "All"},
        {"id": "ongoing", "name": "Ongoing"},
        {"id": "completed", "name": "Completed"},
    ],
}

_MADARA_SORTS = (
    '                        1 => "latest",\n'
    '                        2 => "alphabet",\n'
    '                        3 => "rating",\n'
    '                        4 => "new-manga",\n'
    '                        _ => "views",'
)
_MANGAREADER_SORTS = (
    '                        1 => "update",\n'
    '                        2 => "title",\n'
    '                        3 => "titlereverse",\n'
    '                        4 => "latest",\n'
    '                        _ => "popular",'
)
# How each engine exposes its chapter list: madara may need an extra ajax
# round-trip (takes the key, returns Result), zeistmanga fetches a second
# Blogger feed (returns Result), mangareader has it inline (returns Vec).
# Engines that talk to a JSON API rather than scraping HTML: no Document is
# fetched, and details/pages take keys directly.
API_ENGINES = {"mangafire"}

CHAPTERS_CALL = {
    "madara": "self.0.chapters(&manga.key, &doc)?",
    "mangareader": "self.0.chapters(&doc)",
    "zeistmanga": "self.0.chapters(&doc)?",
    "hotcomics": "self.0.chapters(&doc)",
    "onemanga": "self.0.chapters(&doc)",
    "mmrcms": "self.0.chapters(&doc)",
    "asurascans": "self.0.chapters(&doc)",
    "mangafire": "self.0.chapters(&manga.key)?",
}

SORT_ARMS = {
    "madara": _MADARA_SORTS,
    "mangareader": _MANGAREADER_SORTS,
    "zeistmanga": '                        _ => "published",',
    "hotcomics": '                        _ => "newest",',
    "onemanga": '                        _ => "default",',
    "mmrcms": '                        1 => "latest",\n'
              '                        2 => "alphabetical",\n'
              '                        _ => "popular",',
    "asurascans": '                        1 => "",\n'
                  '                        2 => "rating",\n'
                  '                        3 => "asc",\n'
                  '                        _ => "popular",',
    "mangafire": '                        1 => "latest",\n'
                 '                        _ => "popular",',
}


# Names that would shadow something the generated file imports, or that aren't
# valid Rust identifiers. A source whose name is entirely non-Latin (Thai,
# Arabic, CJK) sanitises to an empty string, so the fallback must not collide
# with the `Source` trait the template imports.
RESERVED = {
    "Source", "Self", "Manga", "Chapter", "Page", "Result", "Listing",
    "ListingProvider", "FilterValue", "MangaPageResult", "String", "Vec",
}


def struct_name(name, ident):
    s = re.sub(r"[^A-Za-z0-9]", "", name)
    if not s or s in RESERVED:
        # derive from the package id instead: "th.sodsaime" -> "ThSodsaime"
        s = "".join(part.capitalize() for part in re.split(r"[^A-Za-z0-9]+", ident) if part)
    if not s:
        s = "NyoraSource"
    if s[0].isdigit():
        s = "S" + s
    if s in RESERVED:
        s += "Src"
    return s


def emit(row, out_dir):
    cfg = row.get("config") or {}
    lang = row.get("lang") or "en"
    sid = row["id"]
    name = row.get("name") or sid
    domain = row["domain"]
    ident = pkg_id(sid, lang)
    cname = crate_name(sid, lang)
    struct = struct_name(name, ident)

    engine = row.get("engine", "madara")
    crate, cfg_ty, src_ty, cfg_fn = SPECS[engine]
    chapters_call = CHAPTERS_CALL[engine]
    # API-backed engines have no document to pass around; everything is keyed.
    if engine in API_ENGINES:
        update_body = (
            "        if needs_details {\n"
            "            self.0.details(&mut manga)?;\n"
            "        }\n"
            "        if needs_chapters {\n"
            f"            manga.chapters = Some({chapters_call});\n"
            "        }\n"
            "        Ok(manga)"
        )
        pages_body = "        self.0.pages(&chapter.key)"
    else:
        update_body = (
            "        let doc = self.0.fetch_manga(&manga.key)?;\n"
            "        if needs_details {\n"
            "            self.0.details(&mut manga, &doc);\n"
            "        }\n"
            "        if needs_chapters {\n"
            f"            manga.chapters = Some({chapters_call});\n"
            "        }\n"
            "        Ok(manga)"
        )
        pages_body = (
            "        let doc = self.0.fetch_chapter(&chapter.key)?;\n"
            "        self.0.pages(&doc)"
        )
    sort_arms = SORT_ARMS[engine]

    d = out_dir / ident
    (d / "src").mkdir(parents=True, exist_ok=True)
    (d / "res").mkdir(parents=True, exist_ok=True)
    (d / ".cargo").mkdir(parents=True, exist_ok=True)

    (d / ".cargo" / "config.toml").write_text(
        '[build]\ntarget = "wasm32-unknown-unknown"\n\n'
        '[target.wasm32-unknown-unknown]\nrunner = "aidoku-test-runner"\n'
    )
    (d / "Cargo.toml").write_text(f"""[package]
name = "{cname}"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dev-dependencies]
aidoku = {{ git = "https://github.com/Aidoku/aidoku-rs", features = ["test"] }}
aidoku-test = {{ git = "https://github.com/Aidoku/aidoku-rs" }}

[dependencies]
aidoku = {{ git = "https://github.com/Aidoku/aidoku-rs" }}
{crate} = {{ path = "../../templates/{engine}" }}

# The release profile is set once at the workspace root (../../Cargo.toml).
# Cargo ignores profiles in non-root members, so it is deliberately omitted here.
""")
    icon = ICONS / f"{ident}.png"
    if icon.exists():
        (d / "res" / "icon.png").write_bytes(icon.read_bytes())

    (d / "res" / "source.json").write_text(json.dumps({
        "info": {
            "id": ident,
            "name": name,
            "version": 1,
            "url": f"https://{domain}",
            "contentRating": 2 if row.get("nsfw") else 0,
            "languages": [lang],
        },
        "listings": LISTINGS.get(engine, [
            {"id": "popular", "name": "Popular"},
            {"id": "latest", "name": "Latest"},
            {"id": "new", "name": "New"},
        ]),
    }, indent=2) + "\n")

    (d / "src" / "lib.rs").write_text(f"""//! {name} ({domain}) — GENERATED by tools/generate.py, do not edit.
//! Source row: nyora-data-driven/repo/madara.json id={sid}

#![no_std]
extern crate alloc;

use aidoku::{{
    alloc::{{String, Vec}},
    prelude::*,
    Chapter, FilterValue, Listing, ListingProvider, Manga, MangaPageResult, Page, Result,
    Source,
}};
use {crate.replace("-", "_")}::{{{cfg_ty}, {src_ty}}};

static CONFIG: {cfg_ty} = {cfg_ty} {{
{cfg_fn(cfg, row)}
}};

struct {struct}({src_ty});

impl Source for {struct} {{
    fn new() -> Self {{
        Self({src_ty}::new(&CONFIG))
    }}

    fn get_search_manga_list(
        &self,
        query: Option<String>,
        page: i32,
        filters: Vec<FilterValue>,
    ) -> Result<MangaPageResult> {{
        // Madara serves a genre as its own archive path, so a genre filter
        // replaces the search transport rather than adding a parameter.
        let mut genre: Option<String> = None;
        let mut sort: Option<&str> = None;
        for f in &filters {{
            match f {{
                FilterValue::Select {{ id, value }} if id == "genre" => {{
                    genre = Some(value.clone())
                }}
                FilterValue::Sort {{ id, index, .. }} if id == "sort" => {{
                    sort = Some(match index {{
{sort_arms}
                    }})
                }}
                _ => {{}}
            }}
        }}
        self.0.search_filtered(query, page, sort, genre.as_deref())
    }}

    fn get_manga_update(
        &self,
        mut manga: Manga,
        needs_details: bool,
        needs_chapters: bool,
    ) -> Result<Manga> {{
{update_body}
    }}

    fn get_page_list(&self, _manga: Manga, chapter: Chapter) -> Result<Vec<Page>> {{
{pages_body}
    }}
}}

impl ListingProvider for {struct} {{
    fn get_manga_list(&self, listing: Listing, page: i32) -> Result<MangaPageResult> {{
        self.0.listing(&listing.id, page)
    }}
}}

register_source!({struct}, ListingProvider);

#[cfg(test)]
mod test {{
    use super::*;
    use aidoku_test::aidoku_test;

    #[aidoku_test]
    fn search_works() {{
        let src = {struct}::new();
        let res = src.get_search_manga_list(None, 1, Vec::new()).expect("search failed");
        assert!(!res.entries.is_empty(), "no entries parsed");
        assert!(!res.entries[0].title.is_empty(), "empty title");
    }}

    /// Walks entries until one has chapters. A single entry proves nothing:
    /// some listings legitimately contain items with no chapters at all (anime
    /// entries on Blogger themes, for one), and asserting on entry 0 reports a
    /// working source as broken.
    #[aidoku_test]
    fn chapters_parse() {{
        let src = {struct}::new();
        let res = src.get_search_manga_list(None, 1, Vec::new()).expect("search failed");
        for m in res.entries.into_iter().take(4) {{
            let full = match src.get_manga_update(m, true, true) {{
                Ok(f) => f,
                Err(_) => continue,
            }};
            if full.chapters.map(|c| !c.is_empty()).unwrap_or(false) {{
                return;
            }}
        }}
        panic!("no entry in the first page had any chapters");
    }}
}}
""")
    return ident


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--id", help="only this source id")
    ap.add_argument("--limit", type=int)
    ap.add_argument("--live-only", action="store_true",
                    help="emit only sources that answered the last liveness sweep "
                         "(data-liveness.json); off by default so the generated set "
                         "matches the Android/aidoku catalogue")
    ap.add_argument("--out", default="sources")
    a = ap.parse_args()

    rows = []
    for eng in ENGINES:
        f = DATA / f"{eng}.json"
        if not f.exists():
            print(f"warning: missing {f}", file=sys.stderr)
            continue
        r = json.loads(f.read_text())
        if not isinstance(r, list):
            r = r.get("sources", [])
        for row in r:
            row.setdefault("engine", eng)
        rows.extend(r)

    sel = [r for r in rows if r.get("domain")]
    if a.live_only:
        sel = [r for r in sel if not r.get("broken")]
    # A row's `engine` field can name a kotatsu engine we haven't ported (e.g.
    # pagedmanga). Skip those rather than crashing the whole run.
    unported = sorted({r.get("engine") for r in sel} - set(SPECS))
    if unported:
        sel = [r for r in sel if r.get("engine") in SPECS]
    dead = 0
    if a.live_only and LIVENESS.exists():
        live = json.loads(LIVENESS.read_text())
        before = len(sel)
        # "cf" = Cloudflare-challenged: curl can't reach it but the app can,
        # so those are kept. Everything else (404, parked, empty, no feed) is a
        # source that cannot work on any client.
        sel = [r for r in sel
               if live.get(f"{r['engine']}:{r['id']}", "200") in ("200", "cf")]
        dead = before - len(sel)
    if a.id:
        sel = [r for r in sel if r["id"] == a.id]
    if a.limit:
        sel = sel[: a.limit]

    out = ROOT / a.out
    made = [emit(r, out) for r in sel]
    print(f"generated {len(made)} source crates in {out}")
    if not a.id and not a.limit:
        skipped = len(rows) - len(sel) - dead
        if a.live_only:
            print(f"skipped {skipped} flagged broken/unported, "
                  f"{dead} more that failed the liveness sweep")
        elif skipped:
            print(f"skipped {skipped} (engine not ported)")
        if unported:
            print(f"engines not yet ported: {', '.join(unported)}")


if __name__ == "__main__":
    main()
