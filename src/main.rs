mod local;
mod remote;
mod sync;

use anyhow::Result;
use bunny_api_tokio::{EdgeStorageClient, edge_storage::Endpoint};
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    author,
    version,
    about = "Sync a local directory to a Bunny.net storage zone"
)]
struct Args {
    /// Local directory to sync
    local_dir: PathBuf,

    /// Bunny.net storage zone API key
    #[arg(long, env = "BUNNY_API_KEY")]
    api_key: String,

    /// Bunny.net storage zone name
    #[arg(long, env = "BUNNY_ZONE")]
    zone: String,

    /// Storage endpoint region
    #[arg(long, env = "BUNNY_ENDPOINT", default_value = "frankfurt", value_parser = parse_endpoint)]
    endpoint: Endpoint,

    /// Print what would be done without making any changes
    #[arg(long)]
    dry_run: bool,
}

fn parse_endpoint(s: &str) -> Result<Endpoint, String> {
    match s.to_lowercase().as_str() {
        "new-york" | "newyork" | "ny" => Ok(Endpoint::NewYork),
        "los-angeles" | "losangeles" | "la" => Ok(Endpoint::LosAngeles),
        "singapore" | "sg" => Ok(Endpoint::Singapore),
        "stockholm" | "se" => Ok(Endpoint::Stockholm),
        "sydney" | "au" => Ok(Endpoint::Sydney),
        "sao-paulo" | "saopaulo" | "br" => Ok(Endpoint::SaoPaulo),
        "johannesburg" | "za" => Ok(Endpoint::Johannesburg),
        "frankfurt" | "de" => Ok(Endpoint::Frankfurt),
        other => Err(format!(
            "unknown endpoint {other:?}; valid values: \
             frankfurt, new-york, los-angeles, singapore, stockholm, \
             sydney, sao-paulo, johannesburg"
        )),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let client = EdgeStorageClient::new(&args.api_key, args.endpoint, &args.zone).await?;

    println!("Scanning local directory: {}", args.local_dir.display());
    let local_files = local::scan(&args.local_dir).await?;
    println!("  Found {} local file(s)", local_files.len());

    println!("Scanning remote storage zone: {}", args.zone);
    let remote_files = remote::scan(&client, &args.zone).await?;
    println!("  Found {} remote file(s)", remote_files.len());

    let plan = sync::SyncPlan::build(&local_files, &remote_files);

    if args.dry_run {
        println!("\nDry-run mode — no changes will be made:");
        plan.print();
    } else {
        plan.print();
        if !plan.uploads.is_empty() || !plan.deletes.is_empty() {
            println!("\nExecuting sync...");
            plan.execute(&client, &args.local_dir).await?;
            println!("Done.");
        }
    }

    Ok(())
}
