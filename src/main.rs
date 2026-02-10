use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
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

        /// Output directory for the ISO file
        /// The ISO will be named: <disc_title>-<disc_artist>-[disc_catalog].iso
        output: PathBuf,

        /// Write disc information to a text file alongside the ISO
        #[arg(short, long)]
        write_info: bool,
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
        Commands::DumpIso { server, output, write_info } => {
            let (ip, port) = parse_server_address(&server)?;

            println!("Connecting to {}:{}...", ip, port);
            let handle = sacd_net_reader::open_network_reader(ip, port)
                .context("Failed to connect to SACD server")?;
            println!("Connected!");

            // Read disc info to generate filename
            println!("Reading disc information...");
            let mut sb_reader = scarletbook::reader::new(handle)
                .context("Failed to read SACD metadata")?;

            // Generate ISO filename from disc metadata
            let title = sb_reader.get_master_text()
                .and_then(|mt| mt.disc_title.as_ref())
                .map(|s| s.clone())
                .unwrap_or_else(|| "Unknown_Title".to_string());

            let artist = sb_reader.get_master_text()
                .and_then(|mt| mt.disc_artist.as_ref())
                .map(|s| s.clone())
                .unwrap_or_else(|| "Unknown_Artist".to_string());

            let catalog = sb_reader.get_master_toc().disc_catalog();
            let catalog = if catalog.is_empty() {
                "Unknown_Catalog".to_string()
            } else {
                catalog
            };

            // Sanitize filename components (remove invalid characters but keep spaces)
            fn sanitize_filename(s: &str) -> String {
                s.chars()
                    .map(|c| match c {
                        '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                        c if c.is_control() => '_',
                        c => c,
                    })
                    .collect::<String>()
                    .trim()
                    .to_string()
            }

            let iso_filename = format!("{}-{}-[{}].iso",
                sanitize_filename(&title),
                sanitize_filename(&artist),
                sanitize_filename(&catalog)
            );

            println!("Disc: {}", iso_filename);

            // Create output directory if it doesn't exist
            if !output.exists() {
                fs::create_dir_all(&output)
                    .context("Failed to create output directory")?;
            }

            // Build full output path
            let output_path = output.join(&iso_filename);
            println!("Output: {}", output_path.display());

            // Write disc info to text file if requested
            if write_info {
                let info_filename = format!("{}-{}-[{}].txt",
                    sanitize_filename(&title),
                    sanitize_filename(&artist),
                    sanitize_filename(&catalog)
                );
                let info_path = output.join(&info_filename);

                println!("Writing disc info to: {}", info_path.display());
                sb_reader.write_disc_info_to_file(&info_path)
                    .context("Failed to write disc info file")?;
            }

            let pb = ProgressBar::new(0);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} sectors ({percent}%) {bytes}/{total_bytes} [{elapsed_precise}]")
                    .unwrap()
                    .progress_chars("#>-")
            );

            sb_reader.get_reader_mut().dump_iso(
                &output_path,
                scarletbook::consts::SACD_LSN_SIZE,
                Some(|current, total| {
                    if pb.length().unwrap_or(0) == 0 {
                        pb.set_length(total as u64);
                    }
                    pb.set_position(current as u64);
                }),
            )?;

            pb.finish_with_message("Complete!");
            println!("ISO dumped successfully to: {}", output_path.display());

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
