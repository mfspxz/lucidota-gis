//! pt-cli — the operator CLI (the flip phone).
//!
//! Compiles to one binary: `palantour serve` runs everything.
//! One artist, one binary, one embedded SurrealDB file.
//! No Docker, no Postgres, no cloud.

#[cfg(test)]
mod e2e_test;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "palantour", about = "Sovereign festival application intelligence")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Start the full pipeline (API + harvester + bridges).
    Serve {
        /// Path to SurrealDB data directory.
        #[arg(long, default_value = "./palantour_data")]
        data_dir: String,
        /// API port.
        #[arg(long, default_value_t = 9753)]
        port: u16,
    },
    /// Harvest a single URL and print results.
    Fetch {
        url: String,
    },
    /// Run ranking on all known events for an artist.
    Rank {
        artist_id: String,
    },
    /// Show the application tracker FSM for an artist.
    Tracker {
        artist_id: String,
    },
    /// Run a scraper directly, list the fleet, or inspect a scraper's controls.
    Scrape {
        /// Scraper id (e.g. TIN_002). Omit with --list to enumerate the fleet.
        id: Option<String>,
        /// Query string passed to the scraper.
        query: Option<String>,
        /// List all compiled+enabled scrapers with their controls and print exit.
        #[arg(long)]
        list: bool,
        /// Describe one scraper's declared limits + resolved controls, then exit.
        #[arg(long)]
        describe: bool,
        /// Path to the scrapers.toml control config.
        #[arg(long, default_value = "scrapers.toml")]
        config: String,
        /// Emit JSON instead of human text.
        #[arg(long)]
        json: bool,
        /// Report whether the headless browser lane is available, then exit.
        #[arg(long)]
        check_browser: bool,
    },
}

pub async fn run(cli: Cli) -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "palantour=info".into()),
        )
        .init();

    match cli.command {
        Command::Serve { data_dir, port } => {
            tracing::info!(data_dir = %data_dir, port = %port, "palantour starting");
            let _db = pt_db::SurrealStore::open(&data_dir).await?;
            tracing::info!("palantour online — operator-sovereign festival intelligence");
            // TODO: start all subsystems
            Ok(())
        }
        Command::Fetch { url } => {
            tracing::info!(url = %url, "fetch");
            Ok(())
        }
        Command::Rank { artist_id } => {
            tracing::info!(artist_id = %artist_id, "rank");
            Ok(())
        }
        Command::Tracker { artist_id } => {
            tracing::info!(artist_id = %artist_id, "tracker");
            Ok(())
        }
        Command::Scrape {
            id,
            query,
            list,
            describe,
            config,
            json,
            check_browser,
        } => run_scrape(id, query, list, describe, config, json, check_browser).await,
    }
}

/// The `scrape` subcommand — the CLI face of the scraper fleet.
/// Routes through `pt_scrapers::dispatch`; reads controls from scrapers.toml.
async fn run_scrape(
    id: Option<String>,
    query: Option<String>,
    list: bool,
    describe: bool,
    config_path: String,
    json: bool,
    check_browser: bool,
) -> anyhow::Result<()> {
    use pt_scrapers::{build_registry, dispatch, ControlConfig, Enablement, ScrapeCtx, ScrapeParams};
    use std::sync::Arc;

    // --check-browser: report browser lane availability and exit.
    if check_browser {
        let compiled = pt_scrapers::lanes::browser::feature_compiled();
        println!(
            "browser lane compiled: {}\n(build with --features social/browser to enable; a Chromium binary is required at runtime)",
            compiled
        );
        return Ok(());
    }

    let cfg = Arc::new(ControlConfig::load(std::path::Path::new(&config_path))?);
    let registry = build_registry();

    // --list: enumerate the fleet with declared limits + resolved controls.
    if list {
        let mut metas = registry.list();
        metas.sort_by(|a, b| a.id.cmp(&b.id));
        if json {
            let rows: Vec<_> = metas
                .iter()
                .map(|m| {
                    serde_json::json!({
                        "id": m.id,
                        "source": m.source_label,
                        "domain": format!("{:?}", m.domain),
                        "archetype": format!("{:?}", m.archetype),
                        "auth_required": m.auth_required,
                        "enabled": cfg.is_enabled(&m.id),
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&rows)?);
        } else {
            println!("{} scrapers compiled:", metas.len());
            for m in &metas {
                let en = if cfg.is_enabled(&m.id) { "on " } else { "OFF" };
                println!(
                    "  [{en}] {:<14} {:<20} {:<11} {}",
                    m.id,
                    format!("{:?}", m.domain),
                    format!("{:?}", m.archetype),
                    m.source_label
                );
            }
        }
        return Ok(());
    }

    let id = match id {
        Some(i) => i,
        None => anyhow::bail!("provide a scraper id, or use --list"),
    };

    let scraper = registry
        .get(&id)
        .ok_or_else(|| anyhow::anyhow!("no such scraper compiled in: {id} (try --list)"))?;

    // --describe: show declared limits + resolved-and-clamped controls, then exit.
    if describe {
        let limits = scraper.limits();
        let domain = scraper.primary_domain().unwrap_or("").to_string();
        let controls = cfg.resolve(&id, &domain, &limits);
        let out = serde_json::json!({
            "id": id,
            "meta": {
                "source": scraper.meta().source_label,
                "domain": format!("{:?}", scraper.meta().domain),
                "archetype": format!("{:?}", scraper.meta().archetype),
                "auth_required": scraper.meta().auth_required,
                "auth_hint": scraper.meta().auth_hint,
            },
            "declared_limits": limits,
            "resolved_controls": controls,
            "default_config": scraper.default_config(),
            "enabled": cfg.is_enabled(&id),
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    // Run it.
    let query = query.unwrap_or_default();
    let ctx = ScrapeCtx::with_config(cfg.clone())?;
    let record = dispatch(&registry, &Enablement::All, &ctx, &id, ScrapeParams::from_query(query)).await;

    if json {
        println!("{}", serde_json::to_string_pretty(&record)?);
    } else {
        println!("scraper : {}", record.scraper_id);
        println!("source  : {}", record.source_label);
        println!("status  : {:?}", record.status);
        println!("notes   : {}", record.notes);
        println!("elapsed : {}ms", record.elapsed_ms);
        println!("candidates ({}):", record.candidates.len());
        for (i, c) in record.candidates.iter().enumerate().take(20) {
            println!("  [{i}] conf={:.2} {}", c.confidence, c.data);
        }
    }
    Ok(())
}
