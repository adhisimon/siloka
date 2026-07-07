use clap::Parser;
mod master;

#[derive(Parser, Debug)]
#[command(author, version, about = "Siloka Object Storage")]
struct Args {
    #[arg(short, long)]
    mode: String, // "master", "worker", atau "storage"
}

fn main() {
    env_logger::init();

    let args = Args::parse();

    match args.mode.as_str() {
        "master" => {
            println!("Starting siloka-master...");
            master::run();
        }
        _ => {
            eprintln!("Unknown component to run: {}", args.mode);
            std::process::exit(1);
        }
    }
}
