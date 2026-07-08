use std::path::PathBuf;
use clap::Parser;
mod master;
mod storage;

#[derive(Parser, Debug)]
#[command(
    name = "siloka",
    author = "Adhidarma a.k.a adhisimon",
    version,
    about = "Siloka Object Storage"
)]
struct Args {
    #[arg(short, long)]
    mode: String, // "master", "worker", atau "storage"
    
    /// Storage base path.
    /// If not provided via CLI, it will look for the SILOKA_STORAGE_PATH env variable.
    /// Default fallback is "./data".
    #[arg(short, long, env = "SILOKA_STORAGE_PATH")]
    path: Option<PathBuf>,
}

fn main() {
    env_logger::init();

    let args = Args::parse();

    let base_path = args.path.unwrap_or_else(|| PathBuf::from("./data"));

    println!("Initializing Siloka Node...");
    println!("Operating Mode : {}", args.mode);
    println!("Storage Path   : {:?}", base_path);

    match args.mode.as_str() {
        "master" => {
            println!("Starting siloka-master...");
            master::run();
        }
        "storage" => {
            println!("Starting siloka-storage...");
            storage::run(base_path);
        }
        _ => {
            eprintln!("Unknown component to run: {}", args.mode);
            std::process::exit(1);
        }
    }
}
