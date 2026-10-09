//! Scraper registry — the catalog of compiled-and-available scrapers.
//!
//! Only scrapers whose cargo feature is enabled are registered. `build_registry`
//! is the single source of truth for what exists in a given build.

use crate::types::{Scraper, ScraperMeta};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Thread-safe map of scraper id -> scraper instance.
pub struct ScraperRegistry {
    scrapers: BTreeMap<String, Arc<dyn Scraper>>,
}

impl ScraperRegistry {
    pub fn new() -> Self {
        Self {
            scrapers: BTreeMap::new(),
        }
    }

    /// Register a scraper (idempotent by id).
    pub fn register(&mut self, scraper: Arc<dyn Scraper>) {
        self.scrapers.insert(scraper.id().to_string(), scraper);
    }

    /// Look up a scraper by id.
    pub fn get(&self, id: &str) -> Option<Arc<dyn Scraper>> {
        self.scrapers.get(id).cloned()
    }

    /// Enumerate metadata for every registered scraper.
    pub fn list(&self) -> Vec<ScraperMeta> {
        self.scrapers.values().map(|s| s.meta()).collect()
    }

    /// Number of registered scrapers.
    pub fn len(&self) -> usize {
        self.scrapers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.scrapers.is_empty()
    }
}

impl Default for ScraperRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Build the registry for this build. Each scraper is gated by its domain feature,
/// so a `--no-default-features --features festival` build only registers festival
/// scrapers.
pub fn build_registry() -> ScraperRegistry {
    #[allow(unused_mut)]
    let mut reg = ScraperRegistry::new();

    // --- bc-osint domain ---
    #[cfg(feature = "bc-osint")]
    {
        reg.register(Arc::new(crate::scrapers::tin_002_orgbook::OrgBook));
        reg.register(Arc::new(crate::scrapers::tin_007_ltsa::LtsaTitle));
        reg.register(Arc::new(crate::scrapers::tin_003_vancouver_opendata::VancouverOpendata));
        reg.register(Arc::new(crate::scrapers::tin_010_van_bizlicence::VanBizLicence));
        reg.register(Arc::new(crate::scrapers::tin_024_corps_canada::CorpsCanada));
        reg.register(Arc::new(crate::scrapers::tin_025_bc_geocoder::BcGeocoder));
        reg.register(Arc::new(crate::scrapers::tin_050_van_city_council::VanCityCouncil));
        reg.register(Arc::new(crate::scrapers::tin_022_bc_assessment::BcAssessment));
        reg.register(Arc::new(crate::scrapers::tin_026_bc_legislature::BcLegislature));
        reg.register(Arc::new(crate::scrapers::tin_027_bc_courts::BcCourts));
        reg.register(Arc::new(crate::scrapers::tin_029_bc_courts::BcCourtsEsearch));
        reg.register(Arc::new(crate::scrapers::tin_034_bc_gazette::BcGazette));
        reg.register(Arc::new(crate::scrapers::tin_041_elections_bc::ElectionsBc));
        reg.register(Arc::new(crate::scrapers::tin_042_elections_bc_donations::ElectionsBcDonations));
        reg.register(Arc::new(crate::scrapers::tin_043_bc_assessment_bulk::BcAssessmentBulk));
        // static-html wave (second half) — bc-osint
        reg.register(Arc::new(crate::scrapers::tin_044_van_dev_permits::VanDevPermits));
        reg.register(Arc::new(crate::scrapers::tin_046_vsa::Vsa));
        reg.register(Arc::new(crate::scrapers::tin_047_bc_supreme_court::BcSupremeCourt));
        reg.register(Arc::new(crate::scrapers::tin_026_shapeyourcity::ShapeYourCity));
        reg.register(Arc::new(crate::scrapers::tin_037_shapeyourcity::ShapeYourCityRezoning));
        reg.register(Arc::new(crate::scrapers::tin_027_bc_ppr::BcPpr));
        reg.register(Arc::new(crate::scrapers::tin_040_metro_van::MetroVan));
        reg.register(Arc::new(crate::scrapers::tin_052_abc_donor_lookup::AbcDonorLookup));
    }

    // --- finance domain ---
    #[cfg(feature = "finance")]
    {
        reg.register(Arc::new(crate::scrapers::mkt_001_edgar::EdgarFilings));
        reg.register(Arc::new(crate::scrapers::mkt_002_short_interest::ShortInterest));
        reg.register(Arc::new(crate::scrapers::mkt_003_options_flow::OptionsFlow));
        reg.register(Arc::new(crate::scrapers::mkt_004_sentiment::SocialSentiment));
        reg.register(Arc::new(crate::scrapers::mkt_005_form4::Form4Insider));
        reg.register(Arc::new(crate::scrapers::mkt_006_ftd::FailureToDeliver));
        reg.register(Arc::new(crate::scrapers::mkt_007_darkpool::DarkPool));
        reg.register(Arc::new(crate::scrapers::mkt_008_13f::Institutional13F));
    }

    // --- general domain ---
    #[cfg(feature = "general")]
    {
        reg.register(Arc::new(crate::scrapers::tin_011_reddit::Reddit));
        reg.register(Arc::new(crate::scrapers::tin_016_wayback::Wayback));
        reg.register(Arc::new(crate::scrapers::tin_017_crtsh::Crtsh));
        reg.register(Arc::new(crate::scrapers::tin_018_whois::Whois));
        reg.register(Arc::new(crate::scrapers::tin_019_opencorporates::OpenCorporates));
        reg.register(Arc::new(crate::scrapers::tin_020_dns::DnsLookup));
        reg.register(Arc::new(crate::scrapers::tin_028_canada_post_fsa::CanadaPostFsa));
        reg.register(Arc::new(crate::scrapers::tin_029_google_news::GoogleNewsRss));
        reg.register(Arc::new(crate::scrapers::tin_048_bbb_complaints::BbbComplaints));
        reg.register(Arc::new(crate::scrapers::tin_049_google_news::GoogleNews));
        reg.register(Arc::new(crate::scrapers::tin_009_craigslist::Craigslist));
        reg.register(Arc::new(crate::scrapers::tin_021_techstack::TechStack));
        reg.register(Arc::new(crate::scrapers::tin_030_cra_bn::CraBn));
        // static-html wave (second half) — general
        reg.register(Arc::new(crate::scrapers::tin_045_craigslist_archive::CraigslistArchive));
        reg.register(Arc::new(crate::scrapers::tin_028_canada411::Canada411));
        reg.register(Arc::new(crate::scrapers::tin_035_youtube::YouTube));
    }

    // --- academic-integrity domain ---
    #[cfg(feature = "academic-integrity")]
    {
        reg.register(Arc::new(crate::scrapers::sci_001_crossref::Crossref));
        reg.register(Arc::new(crate::scrapers::sci_002_retraction_watch::RetractionWatch));
        reg.register(Arc::new(crate::scrapers::sci_003_pubpeer::PubPeer));
        reg.register(Arc::new(crate::scrapers::sci_004_openalex::OpenAlex));
        reg.register(Arc::new(crate::scrapers::sci_005_orcid::Orcid));
        reg.register(Arc::new(crate::scrapers::sci_008_doaj::Doaj));
        reg.register(Arc::new(crate::scrapers::sci_007_tortured_phrases::TorturedPhrases));
    }

    // --- activist domain (non-harvest management/analysis modules) ---
    #[cfg(feature = "activist")]
    {
        reg.register(Arc::new(crate::scrapers::act_001_signal_monitor::SignalMonitor));
        reg.register(Arc::new(crate::scrapers::act_002_legal_observer::LegalObserver));
        reg.register(Arc::new(crate::scrapers::act_003_foia_tracker::FoiaTracker));
        reg.register(Arc::new(crate::scrapers::act_004_event_planner::EventPlanner));
        reg.register(Arc::new(crate::scrapers::act_005_release_tracker::ReleaseTracker));
        reg.register(Arc::new(crate::scrapers::act_007_opsec_auditor::OpsecAuditor));
        reg.register(Arc::new(crate::scrapers::act_008_counter_surveillance::CounterSurveillance));
    }

    // --- social domain (browser lane / anti-bot) ---
    #[cfg(feature = "social")]
    {
        reg.register(Arc::new(crate::scrapers::tin_001::Bcfsa));
        reg.register(Arc::new(crate::scrapers::tin_013::Instagram));
        reg.register(Arc::new(crate::scrapers::tin_014::LinkedIn));
        reg.register(Arc::new(crate::scrapers::tin_015::Facebook));
        reg.register(Arc::new(crate::scrapers::tin_031_airbnb::Airbnb));
        reg.register(Arc::new(crate::scrapers::tin_032_glassdoor::Glassdoor));
        reg.register(Arc::new(crate::scrapers::tin_032_vrbo::Vrbo));
        reg.register(Arc::new(crate::scrapers::tin_033_indeed::Indeed));
        reg.register(Arc::new(crate::scrapers::tin_033_zillow::Zillow));
        reg.register(Arc::new(crate::scrapers::tin_034_twitter::TwitterScrape));
        reg.register(Arc::new(crate::scrapers::tin_036::TikTok));
        reg.register(Arc::new(crate::scrapers::tin_053::Perb));
        reg.register(Arc::new(crate::scrapers::tin_054::Terb));
    }

    // Additional domains register here as scrapers are ported:
    //   #[cfg(feature = "festival")]   { reg.register(...); }
    //   #[cfg(feature = "finance")]    { reg.register(...); }
    //   #[cfg(feature = "social")]     { reg.register(...); }  // browser lane

    reg
}
