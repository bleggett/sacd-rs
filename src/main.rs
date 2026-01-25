use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use std::net::IpAddr;
use std::path::PathBuf;

mod sacd_net_reader;
mod scarletbook;

pub mod sacd_ripper {
    include!(concat!(env!("OUT_DIR"), "/libsacd.sacd_ripper.rs"));
}

#[derive(Parser)]
#[command(name = "sacd-extract")]
#[command(about = "SACD extraction utility", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Dump an ISO image from a network SACD server
    DumpIso {
        /// Server address in format IP:PORT (e.g., 192.168.1.130:2002)
        #[arg(short, long)]
        server: String,

        /// Output ISO file path
        output: PathBuf,
    },
    /// Print disc and track information
    PrintInfo {
        /// Server address in format IP:PORT (e.g., 192.168.1.130:2002)
        #[arg(short, long)]
        server: String,
    },
}

fn parse_server_address(server: &str) -> Result<(IpAddr, u16)> {
    let parts: Vec<&str> = server.split(':').collect();
    if parts.len() != 2 {
        anyhow::bail!("Server address must be in format IP:PORT (e.g., 192.168.1.130:2002)");
    }

    let ip: IpAddr = parts[0]
        .parse()
        .context("Invalid IP address")?;
    let port: u16 = parts[1]
        .parse()
        .context("Invalid port number")?;

    Ok((ip, port))
}

fn main() -> Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::DumpIso { server, output } => {
            let (ip, port) = parse_server_address(&server)?;

            println!("Connecting to {}:{}...", ip, port);
            let mut handle = sacd_net_reader::open_network_reader(ip, port)
                .context("Failed to connect to SACD server")?;
            println!("Connected!");

            let pb = ProgressBar::new(0);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} sectors ({percent}%) {bytes}/{total_bytes} [{elapsed_precise}]")
                    .unwrap()
                    .progress_chars("#>-")
            );

            handle.dump_iso(
                &output,
                scarletbook::consts::SACD_LSN_SIZE,
                Some(|current, total| {
                    if pb.length().unwrap_or(0) == 0 {
                        pb.set_length(total as u64);
                    }
                    pb.set_position(current as u64);
                }),
            )?;

            pb.finish_with_message("Complete!");
            println!("ISO dumped successfully to: {}", output.display());

            Ok(())
        }
        Commands::PrintInfo { server } => {
            let (ip, port) = parse_server_address(&server)?;

            println!("Connecting to {}:{}...", ip, port);
            let handle = sacd_net_reader::open_network_reader(ip, port)
                .context("Failed to connect to SACD server")?;
            println!("Connected!");

            let mut sb_reader = scarletbook::reader::new(handle)
                .context("Failed to read SACD metadata")?;

            sb_reader.print_disc_info();

            Ok(())
        }
    }
}
