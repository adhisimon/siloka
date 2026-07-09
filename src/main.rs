mod master;
mod storage;
mod logger;

use clap::Parser;
use std::path::PathBuf;

/// CLI Arguments structure for Siloka using Clap.
#[derive(Parser, Debug)]
#[command(
    name = "siloka",
    author = "Adhidarma a.k.a adhisimon",
    version,
    about = "Siloka Object Storage"
)]
struct Args {
    #[arg(short, long)]
    mode: String,
    
    #[arg(short = 'd', long, env = "SILOKA_DATA_DIR", default_value = "data")]
    data_dir: PathBuf,

    #[arg(long, env = "SILOKA_BIND_IP", default_value = "0.0.0.0")]
    bind_ip: String,

    #[arg(long, env = "SILOKA_BIND_PORT", default_value = "9111")]
    bind_port: u16,

    #[arg(long, env = "SILOKA_APIKEY", required = true)]
    apikey: String,
}

#[tokio::main]
async fn main() {
    // 1. Initialize tracing via the dedicated logger module.
    logger::init();

    // 2. Parse arguments.
    let args = Args::parse();

    // 3. Resolve data directory: join with CWD only if path is relative.
    let data_dir = if args.data_dir.is_absolute() {
        args.data_dir
    } else {
        std::env::current_dir()
            .expect("Failed to read current working directory")
            .join(args.data_dir)
    };

    // Menggunakan display (%data_dir) agar path dikirim sebagai string bersih ke JSON
    // alih-alih debug format (?data_dir) yang menambahkan escaped quotes.
    tracing::info!(
        mode = args.mode,
        data_dir = %data_dir.display(),
        "Initializing Siloka Node..."
    );

    match args.mode.as_str() {
        "master" => {
            tracing::info!("Starting siloka-master...");
            master::run();
        }
        "storage" => {
            tracing::info!("Starting siloka-storage...");
            
            let bind_address = format!("{}:{}", args.bind_ip, args.bind_port);
            let addr: std::net::SocketAddr = bind_address
                .parse()
                .expect("Failed to parse IP and Port");

            if let Err(e) = storage::start_server(data_dir, addr, args.apikey).await {
                tracing::error!(error = %e, "Storage server encountered a fatal error");
                std::process::exit(1);
            }
        }
        _ => {
            tracing::error!(mode = args.mode, "Unknown component to run");
            std::process::exit(1);
        }
    }
}