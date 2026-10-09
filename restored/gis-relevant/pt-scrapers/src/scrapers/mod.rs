//! Scraper organelles — one module + one feature per scraper.
//!
//! Each module is `#[cfg]`-gated by its domain feature so removing a feature
//! removes the code and its deps. Modules depend only on lanes + pt-core, never
//! on each other.

// --- bc-osint domain ---
#[cfg(feature = "bc-osint")]
pub mod tin_002_orgbook;
#[cfg(feature = "bc-osint")]
pub mod tin_007_ltsa;
#[cfg(feature = "bc-osint")]
pub mod tin_003_vancouver_opendata;
#[cfg(feature = "bc-osint")]
pub mod tin_010_van_bizlicence;
#[cfg(feature = "bc-osint")]
pub mod tin_024_corps_canada;
#[cfg(feature = "bc-osint")]
pub mod tin_025_bc_geocoder;
#[cfg(feature = "bc-osint")]
pub mod tin_050_van_city_council;
#[cfg(feature = "bc-osint")]
pub mod tin_022_bc_assessment;
#[cfg(feature = "bc-osint")]
pub mod tin_026_bc_legislature;
#[cfg(feature = "bc-osint")]
pub mod tin_027_bc_courts;
#[cfg(feature = "bc-osint")]
pub mod tin_029_bc_courts;
#[cfg(feature = "bc-osint")]
pub mod tin_034_bc_gazette;
#[cfg(feature = "bc-osint")]
pub mod tin_041_elections_bc;
#[cfg(feature = "bc-osint")]
pub mod tin_042_elections_bc_donations;
#[cfg(feature = "bc-osint")]
pub mod tin_043_bc_assessment_bulk;
// static-html wave (second half) — bc-osint
#[cfg(feature = "bc-osint")]
pub mod tin_044_van_dev_permits;
#[cfg(feature = "bc-osint")]
pub mod tin_046_vsa;
#[cfg(feature = "bc-osint")]
pub mod tin_047_bc_supreme_court;
#[cfg(feature = "bc-osint")]
pub mod tin_026_shapeyourcity;
#[cfg(feature = "bc-osint")]
pub mod tin_037_shapeyourcity;
#[cfg(feature = "bc-osint")]
pub mod tin_027_bc_ppr;
#[cfg(feature = "bc-osint")]
pub mod tin_040_metro_van;
#[cfg(feature = "bc-osint")]
pub mod tin_052_abc_donor_lookup;

// --- finance domain ---
#[cfg(feature = "finance")]
pub mod mkt_001_edgar;
#[cfg(feature = "finance")]
pub mod mkt_002_short_interest;
#[cfg(feature = "finance")]
pub mod mkt_003_options_flow;
#[cfg(feature = "finance")]
pub mod mkt_004_sentiment;
#[cfg(feature = "finance")]
pub mod mkt_005_form4;
#[cfg(feature = "finance")]
pub mod mkt_006_ftd;
#[cfg(feature = "finance")]
pub mod mkt_007_darkpool;
#[cfg(feature = "finance")]
pub mod mkt_008_13f;

// --- general domain ---
#[cfg(feature = "general")]
pub mod tin_011_reddit;
#[cfg(feature = "general")]
pub mod tin_016_wayback;
#[cfg(feature = "general")]
pub mod tin_017_crtsh;
#[cfg(feature = "general")]
pub mod tin_018_whois;
#[cfg(feature = "general")]
pub mod tin_019_opencorporates;
#[cfg(feature = "general")]
pub mod tin_020_dns;
#[cfg(feature = "general")]
pub mod tin_028_canada_post_fsa;
#[cfg(feature = "general")]
pub mod tin_029_google_news;
#[cfg(feature = "general")]
pub mod tin_048_bbb_complaints;
#[cfg(feature = "general")]
pub mod tin_049_google_news;
#[cfg(feature = "general")]
pub mod tin_009_craigslist;
#[cfg(feature = "general")]
pub mod tin_021_techstack;
#[cfg(feature = "general")]
pub mod tin_030_cra_bn;
// static-html wave (second half) — general
#[cfg(feature = "general")]
pub mod tin_045_craigslist_archive;
#[cfg(feature = "general")]
pub mod tin_028_canada411;
#[cfg(feature = "general")]
pub mod tin_035_youtube;

// --- academic-integrity domain ---
#[cfg(feature = "academic-integrity")]
pub mod sci_001_crossref;
#[cfg(feature = "academic-integrity")]
pub mod sci_002_retraction_watch;
#[cfg(feature = "academic-integrity")]
pub mod sci_003_pubpeer;
#[cfg(feature = "academic-integrity")]
pub mod sci_004_openalex;
#[cfg(feature = "academic-integrity")]
pub mod sci_005_orcid;
#[cfg(feature = "academic-integrity")]
pub mod sci_008_doaj;
#[cfg(feature = "academic-integrity")]
pub mod sci_007_tortured_phrases;

// --- activist domain (non-harvest management/analysis modules) ---
#[cfg(feature = "activist")]
pub mod act_001_signal_monitor;
#[cfg(feature = "activist")]
pub mod act_002_legal_observer;
#[cfg(feature = "activist")]
pub mod act_003_foia_tracker;
#[cfg(feature = "activist")]
pub mod act_004_event_planner;
#[cfg(feature = "activist")]
pub mod act_005_release_tracker;
#[cfg(feature = "activist")]
pub mod act_007_opsec_auditor;
#[cfg(feature = "activist")]
pub mod act_008_counter_surveillance;

// --- social domain (browser lane / anti-bot) ---
#[cfg(feature = "social")]
pub mod tin_001;
#[cfg(feature = "social")]
pub mod tin_013;
#[cfg(feature = "social")]
pub mod tin_014;
#[cfg(feature = "social")]
pub mod tin_015;
#[cfg(feature = "social")]
pub mod tin_031_airbnb;
#[cfg(feature = "social")]
pub mod tin_032_glassdoor;
#[cfg(feature = "social")]
pub mod tin_032_vrbo;
#[cfg(feature = "social")]
pub mod tin_033_indeed;
#[cfg(feature = "social")]
pub mod tin_033_zillow;
#[cfg(feature = "social")]
pub mod tin_034_twitter;
#[cfg(feature = "social")]
pub mod tin_036;
#[cfg(feature = "social")]
pub mod tin_053;
#[cfg(feature = "social")]
pub mod tin_054;

// Ported scrapers register their module here under the appropriate feature gate.
