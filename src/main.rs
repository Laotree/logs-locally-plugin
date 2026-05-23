mod config;
mod db;
mod parser;
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = config::Config::load(Some(&cli.config))
        .context("failed to load config")?;

    match cli.command {
        Commands::Import { file } => {
            let db = db::Db::open(&cfg.db_path)?;

            let jsonl_path = if let Some(path) = file {
                path
            } else {
                let cwd = std::env::current_dir().context("getting current directory")?;
                parser::find_latest_session(&cfg.claude_projects_dir, &cwd)?
                    .context("no session files found for this project")?
            };

            let imported = parser::import_session(&db, &jsonl_path)?;
            if imported {
                println!("Imported: {}", jsonl_path.display());
            } else {
                println!("Already imported (skipped): {}", jsonl_path.display());
            }
        }

        Commands::Serve { port } => {
            let db = db::Db::open(&cfg.db_path)?;
            let addr = format!("{}:{}", cfg.host, port.unwrap_or(cfg.port));
            let listener = tokio::net::TcpListener::bind(&addr)
                .await
                .context(format!("binding to {}", addr))?;

            let app = server::router(db);

            println!("Logs Locally Plugin web server running at http://{}", addr);
            println!("Open your browser and start browsing Claude Code session logs!");

            axum::serve(listener, app)
                .await
                .context("starting server")?;
        }

        Commands::ImportAll { project_dir } => {
            let db = db::Db::open(&cfg.db_path)?;
            let project_dir_name = config::Config::project_dir_name(&project_dir);
            let sessions_dir = cfg.claude_projects_dir.join(&project_dir_name);

            if !sessions_dir.exists() {
                anyhow::bail!("sessions directory not found: {:?}", sessions_dir);
            }

            let mut count = 0;
            let mut entries: Vec<_> = std::fs::read_dir(&sessions_dir)
                .context("reading sessions directory")?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("jsonl"))
                .collect();

            entries.sort();

            for path in &entries {
                match parser::import_session(&db, path) {
                    Ok(true) => {
                        println!("Imported: {}", path.display());
                        count += 1;
                    }
                    Ok(false) => {} // skip already imported
                    Err(e) => {
                        eprintln!("Error importing {:?}: {}", path, e);
                    }
                }
            }

            println!("Done. Imported {} new session(s).", count);
        }
    }

    Ok(())
}
