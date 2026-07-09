use std::path::PathBuf;
use clap::Parser;

mod master;
mod storage;

/// CLI Arguments structure for Siloka using Clap.
/// Defines the command-line interface and maps environment variables.
#[derive(Parser, Debug)]
#[command(
    name = "siloka",
    author = "Adhidarma a.k.a adhisimon",
    version,
    about = "Siloka Object Storage"
)]
struct Args {
    /// Operating mode: "master", "worker", or "storage"
    #[arg(short, long)]
    mode: String,
    
    /// Directory where Siloka stores its physical data.
    /// If not provided via CLI, looks for the SILOKA_DATA_DIR env variable.
    /// Default fallback is "data" (internally resolved as an absolute path from CWD).
    #[arg(short = 'd', long, env = "SILOKA_DATA_DIR", default_value = "data")]
    data_dir: PathBuf,

    /// IP Address to bind the storage HTTP server.
    /// Default fallback is "0.0.0.0" to listen on all network interfaces.
    #[arg(long, env = "SILOKA_BIND_IP", default_value = "0.0.0.0")]
    bind_ip: String,

    /// Port to bind the storage HTTP server.
    /// Default fallback is "9111" (Port 9110 is reserved for Master).
    #[arg(long, env = "SILOKA_BIND_PORT", default_value = "9111")]
    bind_port: u16,

    /// Required API Key for securing PUT/GET/DELETE operations.
    /// Must be provided via --apikey or SILOKA_APIKEY environment variable.
    #[arg(long, env = "SILOKA_APIKEY", required = true)]
    apikey: String,
}

#[tokio::main]
async fn main() {
    // Initialize env_logger for system logging capabilities
    env_logger::init();

    // Parse arguments from CLI / ENV.
    // Since 'apikey' is marked as required, Clap will automatically exit and
    // show a friendly error message to the user if it is missing.
    let args = Args::parse();

    // Resolve the current working directory to guarantee an absolute path
    let cwd = std::env::current_dir()
        .expect("Failed to read the current working directory from the operating system");

    // Rust's PathBuf::join safely handles absolute arguments by returning them directly,
    // while appending relative arguments (like "data") to the CWD base.
    let data_dir = cwd.join(args.data_dir);

    println!("Initializing Siloka Node...");
    println!("Operating Mode : {}", args.mode);
    println!("Data Directory : {:?}", data_dir);

    match args.mode.as_str() {
        "master" => {
            println!("Starting siloka-master...");
            master::run();
        }
        "storage" => {
            println!("Starting siloka-storage...");
            
            // Construct socket address from bind_ip and bind_port
            let bind_address = format!("{}:{}", args.bind_ip, args.bind_port);
            let addr: std::net::SocketAddr = bind_address
                .parse()
                .expect("Failed to parse IP and Port into a valid SocketAddr");

            // Run HTTP storage server asynchronously.
            // Pass the API Key as a guaranteed String.
            if let Err(e) = storage::start_server(data_dir, addr, args.apikey).await {
                eprintln!("Storage server encountered a fatal error: {}", e);
                std::process::exit(1);
            }
        }
        _ => {
            eprintln!("Unknown component to run: {}", args.mode);
            std::process::exit(1);
        }
    }
}