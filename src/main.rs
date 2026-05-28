mod chart;
mod config;
mod db;
mod parser;
mod scorer;
mod scrub;
mod server;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "llp", about = "Logs Locally Plugin — store and browse Claude Code session logs")]
struct Cli {
    /// Path to config file
    #[arg(short, long, default_value = "config.json")]
    config: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Import the latest Claude Code session into SQLite.
    /// Designed to be run from Claude Code's onStop hook.
    Import {
        /// Optional: path to a specific JSONL file to import.
        /// If omitted, auto-detects the latest session for the current project.
        file: Option<PathBuf>,
    },
    /// Start the local web server for browsing logs.
    Serve {
        /// Override the port from config
        #[arg(short, long)]
        port: Option<u16>,
    },
    /// Import all existing sessions from a project.
    ImportAll {
        /// Optional: project directory path (defaults to current working directory)
        #[arg(default_value = ".")]
        project_dir: PathBuf,
    },
    /// Score (or re-score) all sessions in the database that don't yet have a score.
    /// Useful after upgrading from a version that didn't include session scoring.
    Rescore,
    /// Push daily aggregated activity (session + token counts) to a remote llp server.
    /// Only aggregates are sent — no raw session content, titles, or messages.
    Push {
        /// URL of the remote server, e.g. https://your-app.fly.dev
        url: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = config::Config::load(Some(&cli.config));

    match cli.command {
        Commands::Import { file } => {
            let cwd = std::env::current_dir().context("getting current directory")?;

            let jsonl_path = if let Some(path) = file {
                path
            } else {
                parser::find_latest_session(&cfg.claude_projects_dir, &cwd)?
                    .context("no Claude session files found for this project")?
            };

            let mut any_imported = false;
            for db_path in cfg.effective_db_paths() {
                let db = db::Db::open(db_path)?;
                match parser::import_session(&db, &jsonl_path) {
                    Ok(true) => {
                        println!("Imported to {}: {}", db_path.display(), jsonl_path.display());
                        any_imported = true;
                    }
                    Ok(false) => {
                        println!("Already imported (skipped) in {}", db_path.display());
                    }
                    Err(e) => {
                        eprintln!("Error importing to {}: {}", db_path.display(), e);
                    }
                }
            }

            // Also import the latest pi session for this project, if configured.
            if let Some(ref pi_dir) = cfg.pi_jsonl_dir {
                match parser::find_latest_pi_session(pi_dir, &cwd) {
                    Ok(Some(pi_path)) => {
                        for db_path in cfg.effective_db_paths() {
                            let db = db::Db::open(db_path)?;
                            match parser::import_pi_session(&db, &pi_path) {
                                Ok(true) => {
                                    println!("Imported pi session to {}: {}", db_path.display(), pi_path.display());
                                    any_imported = true;
                                }
                                Ok(false) => {}
                                Err(e) => {
                                    eprintln!("Error importing pi session: {}", e);
                                }
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => eprintln!("Warning: could not search pi sessions: {}", e),
                }
            }

            if !any_imported {
                println!("No new data imported.");
            }
        }

        Commands::Serve { port } => {
            let db = db::Db::open(cfg.primary_db_path())?;
            let addr = format!("{}:{}", cfg.host, port.unwrap_or(cfg.port));
            let listener = tokio::net::TcpListener::bind(&addr)
                .await
                .context(format!("binding to {}", addr))?;

            // Push token: env var takes precedence over config
            let push_token = std::env::var("LLP_PUSH_TOKEN")
                .ok()
                .or_else(|| cfg.push_token.clone())
                .unwrap_or_default();

            // Data persistence directory for activity.json
            let data_path = std::env::var("LLP_DATA_DIR")
                .ok()
                .or_else(|| cfg.data_dir.clone())
                .map(|d| std::path::PathBuf::from(d).join("activity.json"));

            let app = server::router(db, server::RouterConfig { push_token, data_path });

            println!("Logs Locally Plugin web server running at http://{}", addr);
            println!("Open your browser and start browsing Claude Code session logs!");

            axum::serve(listener, app)
                .await
                .context("starting server")?;
        }

        Commands::ImportAll { project_dir } => {
            let project_dir_name = config::Config::project_dir_name(&project_dir);
            let sessions_dir = cfg.claude_projects_dir.join(&project_dir_name);

            let mut claude_entries: Vec<_> = if sessions_dir.exists() {
                std::fs::read_dir(&sessions_dir)
                    .context("reading Claude sessions directory")?
                    .filter_map(|e| e.ok())
                    .map(|e| e.path())
                    .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("jsonl"))
                    .collect()
            } else {
                Vec::new()
            };
            claude_entries.sort();

            let pi_entries: Vec<_> = if let Some(ref pi_dir) = cfg.pi_jsonl_dir {
                parser::list_pi_session_files(pi_dir, &project_dir)?
            } else {
                Vec::new()
            };

            if claude_entries.is_empty() && pi_entries.is_empty() {
                anyhow::bail!(
                    "no session files found for {:?} (checked Claude: {:?}, pi: {:?})",
                    project_dir,
                    sessions_dir,
                    cfg.pi_jsonl_dir
                );
            }

            let dbs: Vec<(PathBuf, db::Db)> = cfg
                .effective_db_paths()
                .into_iter()
                .cloned()
                .map(|p| db::Db::open(&p).map(|db| (p, db)))
                .collect::<Result<_>>()?;
            let mut count = 0;

            for path in &claude_entries {
                let mut imported_to_any = false;
                for (db_path, db) in &dbs {
                    match parser::import_session(db, path) {
                        Ok(true) => {
                            println!("Imported to {}: {}", db_path.display(), path.display());
                            imported_to_any = true;
                        }
                        Ok(false) => {}
                        Err(e) => {
                            eprintln!("Error importing {} to {}: {}", path.display(), db_path.display(), e);
                        }
                    }
                }
                if imported_to_any { count += 1; }
            }

            for path in &pi_entries {
                let mut imported_to_any = false;
                for (db_path, db) in &dbs {
                    match parser::import_pi_session(db, path) {
                        Ok(true) => {
                            println!("Imported pi session to {}: {}", db_path.display(), path.display());
                            imported_to_any = true;
                        }
                        Ok(false) => {}
                        Err(e) => {
                            eprintln!("Error importing pi session {} to {}: {}", path.display(), db_path.display(), e);
                        }
                    }
                }
                if imported_to_any { count += 1; }
            }

            println!("Done. Imported {} new session(s).", count);
        }

        Commands::Push { url } => {
            let token = std::env::var("LLP_PUSH_TOKEN")
                .ok()
                .or_else(|| cfg.push_token.clone())
                .unwrap_or_default();
            if token.is_empty() {
                anyhow::bail!(
                    "pushToken not configured. Set `pushToken` in config.json \
                     or export LLP_PUSH_TOKEN=<secret>"
                );
            }

            let db = db::Db::open(cfg.primary_db_path())?;
            // Fetch only aggregated daily counts — no raw content
            let records = db.get_daily_activity(None)?;
            let days: Vec<chart::DayRecord> = records
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect();
            let count = days.len();

            // Render SVG locally so the remote server (CF Worker or self-hosted)
            // never needs to know about raw session data.
            let activity = chart::ActivityData {
                days: days.clone(),
                updated_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
            };
            let svg = chart::render_svg(&activity);

            let client = reqwest::Client::new();
            let target = format!("{}/api/push", url.trim_end_matches('/'));
            let resp = client
                .post(&target)
                .bearer_auth(&token)
                // svg: for CF Worker (store-and-serve)
                // days: for self-hosted server (dashboard re-render)
                .json(&serde_json::json!({ "svg": svg, "days": days }))
                .send()
                .await
                .context("sending push request")?;

            if resp.status().is_success() {
                println!("Pushed {count} day(s) of aggregated activity to {url}");
            } else {
                anyhow::bail!("push failed: HTTP {}", resp.status());
            }
        }

        Commands::Rescore => {
            let db = db::Db::open(cfg.primary_db_path())?;
            let ids = db.get_all_session_ids()?;

            if ids.is_empty() {
                println!("No sessions found.");
                return Ok(());
            }

            println!("Rescoring {} session(s)...", ids.len());
            let mut scored = 0;
            let mut skipped = 0;
            for id in &ids {
                if let Some(session) = db.get_session(id)? {
                    let messages = db.get_messages(id)?;
                    match scorer::score_session(&session, &messages) {
                        Some(score) => {
                            db.upsert_score(&score)?;
                            scored += 1;
                        }
                        None => {
                            db.delete_score(id)?;
                            skipped += 1;
                        }
                    }
                }
            }
            println!("Done. Scored: {}, Trivial (N/A): {}.", scored, skipped);
        }
    }

    Ok(())
}
