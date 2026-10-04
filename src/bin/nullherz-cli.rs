use clap::{Parser, Subcommand};
use std::path::Path;

#[derive(Parser)]
#[command(name = "nullherz-cli")]
#[command(about = "Nullherz Core System & Storage Management CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show status of storage directory, configuration, and installed sidecars
    Status,
    /// List installed sidecars in storage/sidecars/
    ListSidecars,
    /// Purge transient database and state files
    Purge,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Status => {
            println!("━━━ Nullherz Storage & Runtime Status ━━━");
            println!("Storage Root: storage/");
            let cfg_path = Path::new("storage/system_config.json");
            println!("System Config: {}", if cfg_path.exists() { "EXISTS (storage/system_config.json)" } else { "NOT FOUND" });

            let redb_path = Path::new("storage/db/library.redb");
            println!("redb Library Database: {}", if redb_path.exists() { "EXISTS (storage/db/library.redb)" } else { "NOT FOUND" });

            let sqlite_path = Path::new("storage/db/library.db");
            println!("SQLite Relational Database: {}", if sqlite_path.exists() { "EXISTS (storage/db/library.db)" } else { "NOT FOUND" });

            let sidecar_dir = Path::new("storage/sidecars");
            let sidecar_count = if sidecar_dir.exists() {
                std::fs::read_dir(sidecar_dir).map(|entries| entries.count()).unwrap_or(0)
            } else {
                0
            };
            println!("Installed Sidecars Count: {}", sidecar_count);
        }
        Commands::ListSidecars => {
            println!("━━━ Installed Sidecars (storage/sidecars) ━━━");
            let sidecar_dir = Path::new("storage/sidecars");
            if sidecar_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(sidecar_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_file() {
                            println!("  • {}", path.file_name().unwrap_or_default().to_string_lossy());
                        }
                    }
                }
            } else {
                println!("No sidecars directory found.");
            }
        }
        Commands::Purge => {
            println!("🧹 Purging transient state in storage/...");
            let targets = [
                "storage/system_config.json",
                "storage/graph.json",
                "storage/autosave.json",
                "storage/autosave.rkyv",
                "storage/db/library.redb",
                "storage/db/library.db",
                "storage/db/library.db-journal",
            ];
            for t in &targets {
                if Path::new(t).exists() {
                    let _ = std::fs::remove_file(t);
                    println!("  ✓ Removed {}", t);
                }
            }
            println!("Done.");
        }
    }
}
