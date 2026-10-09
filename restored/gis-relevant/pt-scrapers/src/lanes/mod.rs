//! Archetype lanes — shared fetch/parse helpers scrapers compose.
//!
//! A scraper picks a lane for its archetype and never re-implements fetching:
//!   - `api`     — JSON/REST GET/POST + serde_json path extraction
//!   - `html`    — reqwest + `scraper` CSS selectors (regex fallback)
//!   - `feed`    — RSS/Atom/sitemap via feed-rs + sitemap
//!   - `browser` — headless CDP + stealth (feature = "browser")

pub mod api;
pub mod browser;
pub mod feed;
pub mod html;
