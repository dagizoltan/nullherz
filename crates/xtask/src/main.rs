use clap::{Parser, Subcommand};
use std::process::Command;
use std::path::Path;
use std::fs;

#[derive(Parser)]
#[command(name = "cargo-xtask")]
#[command(about = "Automation tasks for Nullherz development workflow", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Launch the Nullherz application (nullherz-inspector)
    Up {
        /// Optional backend override (e.g. alsa, pipewire, mock)
        #[arg(short, long)]
        backend: Option<String>,
    },
    /// Launch the standalone Nullherz local store & gateway server
    #[command(name = "store-up", alias = "store:up")]
    StoreUp,
    /// Purge transient database, autosave, and config files to clear state
    Purge,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Up { backend } => {
            println!("🚀 Starting Nullherz Inspector...");
            let mut cmd = Command::new("cargo");
            cmd.arg("run").arg("-p").arg("nullherz-inspector").arg("--bin").arg("nullherz-inspector");

            if let Some(b) = backend {
                cmd.env("NULLHERZ_BACKEND", b);
            }

            let status = cmd.status().expect("Failed to execute cargo run --bin nullherz-inspector");
            if !status.success() {
                eprintln!("❌ Application exited with status: {}", status);
                std::process::exit(status.code().unwrap_or(1));
            }
        }
        Commands::StoreUp => {
            println!("🛒 Launching Nullherz Store Server...");
            let mut cmd = Command::new("cargo");
            cmd.arg("run").arg("-p").arg("nullherz").arg("--bin").arg("nullherz-server");

            let status = cmd.status().expect("Failed to execute cargo run --bin nullherz-server");
            if !status.success() {
                eprintln!("❌ Store Server exited with status: {}", status);
                std::process::exit(status.code().unwrap_or(1));
            }
        }
        Commands::Purge => {
            println!("🧹 Purging Nullherz database and transient state files...");
            let targets = [
                "system_config.json",
                "library.redb",
                "autosave.json",
                "autosave.rkyv",
                "preferences.json",
                "graph.json",
                "storage/system_config.json",
                "storage/graph.json",
                "storage/autosave.json",
                "storage/autosave.rkyv",
                "storage/db/library.redb",
                "storage/db/library.db",
                "storage/db/library.db-journal",
            ];

            let mut removed_count = 0;
            for target in &targets {
                if Path::new(target).exists() {
                    if let Err(e) = fs::remove_file(target) {
                        eprintln!("  ⚠️  Failed to remove {}: {}", target, e);
                    } else {
                        println!("  ✓ Removed {}", target);
                        removed_count += 1;
                    }
                }
            }

            println!("✨ Purge complete. {} file(s) removed.", removed_count);
        }
    }
}
