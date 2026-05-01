use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::net::IpAddr;
use std::path::PathBuf;

mod sacd_reader;
mod scarletbook;
mod dst_decoder;

use sacd_reader::IsoReader;
use sacd_reader::NetReader;

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
    /// Extract DSF files from an SACD ISO image
    Extract {
        /// Path to the SACD ISO file
        #[arg(short, long)]
        iso: PathBuf,

        /// Output directory for extracted DSF files
        output: PathBuf,

        /// Extract 2-channel tracks
        #[arg(long)]
        stereo: bool,

        /// Extract multi-channel tracks
        #[arg(long)]
        multi_channel: bool,

        /// Select specific tracks to extract (e.g., "1,2,5" or "1-3,5")
        #[arg(short, long)]
        tracks: Option<String>,
    },
    /// Extract DSF files directly from a network SACD server (no ISO needed)
    ExtractNet {
        /// Server address in format IP:PORT (e.g., 192.168.1.130:2002)
        #[arg(short, long)]
        server: String,

        /// Output directory for extracted DSF files
        output: PathBuf,

        /// Extract 2-channel tracks
        #[arg(long)]
        stereo: bool,

        /// Extract multi-channel tracks
        #[arg(long)]
        multichannel: bool,

        /// Select specific tracks to extract (e.g., "1,2,5" or "1-3,5")
        #[arg(short, long)]
        tracks: Option<String>,
    },
}

fn parse_server_address(server: &str) -> Result<(IpAddr, u16)> {
    let parts: Vec<&str> = server.split(':').collect();
    if parts.len() != 2 {
        anyhow::bail!("Server address must be in format IP:PORT (e.g., 192.168.1.130:2002)");
    }

    let ip: IpAddr = parts[0].parse().context("Invalid IP address")?;
    let port: u16 = parts[1].parse().context("Invalid port number")?;

    Ok((ip, port))
}

fn main() -> Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::DumpIso {
            server,
            output,
            write_info,
        } => {
            let (ip, port) = parse_server_address(&server)?;

            println!("Connecting to {}:{}...", ip, port);
            let handle = NetReader::open_network_reader(ip, port)
                .context("Failed to connect to SACD server")?;
            println!("Connected!");

            // Read disc info to generate filename
            println!("Reading disc information...");
            let mut sb_reader =
                scarletbook::reader::new(handle).context("Failed to read SACD metadata")?;

            // Generate ISO filename from disc metadata
            let title = sb_reader
                .get_master_text()
                .and_then(|mt| mt.disc_title.as_ref())
                .cloned()
                .unwrap_or_else(|| "Unknown_Title".to_string());

            let artist = sb_reader
                .get_master_text()
                .and_then(|mt| mt.disc_artist.as_ref())
                .cloned()
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

            let iso_filename = format!(
                "{}-{}-[{}].iso",
                sanitize_filename(&title),
                sanitize_filename(&artist),
                sanitize_filename(&catalog)
            );

            println!("Disc: {}", iso_filename);

            // Create output directory if it doesn't exist
            if !output.exists() {
                fs::create_dir_all(&output).context("Failed to create output directory")?;
            }

            // Build full output path
            let output_path = output.join(&iso_filename);
            println!("Output: {}", output_path.display());

            // Write disc info to text file if requested
            if write_info {
                let info_filename = format!(
                    "{}-{}-[{}].txt",
                    sanitize_filename(&title),
                    sanitize_filename(&artist),
                    sanitize_filename(&catalog)
                );
                let info_path = output.join(&info_filename);

                println!("Writing disc info to: {}", info_path.display());
                sb_reader
                    .write_disc_info_to_file(&info_path)
                    .context("Failed to write disc info file")?;
            }

            let pb = ProgressBar::new(0);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} frames ({percent}%) [{elapsed_precise}]")
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
            let handle = NetReader::open_network_reader(ip, port)
                .context("Failed to connect to SACD server")?;
            println!("Connected!");

            let mut sb_reader =
                scarletbook::reader::new(handle).context("Failed to read SACD metadata")?;

            sb_reader.print_disc_info();

            Ok(())
        }
        Commands::Extract {
            iso,
            output,
            stereo,
            multi_channel,
            tracks,
        } => {
            // Validate that the ISO file exists
            if !iso.exists() {
                anyhow::bail!("ISO file not found: {}", iso.display());
            }

            // If neither flag is specified, extract both
            let extract_stereo = if !stereo && !multi_channel {
                true // Default: extract both
            } else {
                stereo
            };
            let extract_mch = if !stereo && !multi_channel {
                true // Default: extract both
            } else {
                multi_channel
            };

            println!("Opening SACD ISO: {}", iso.display());
            let iso_reader = IsoReader::open(&iso).context("Failed to open ISO file")?;

            println!("Reading disc metadata...");
            let mut sb_reader = scarletbook::reader::new(iso_reader)
                .context("Failed to read SACD metadata from ISO")?;

            // Print disc info
            sb_reader.print_disc_info();

            // Parse track selection
            let selected_tracks = if let Some(track_str) = tracks {
                parse_track_selection(&track_str)?
            } else {
                Vec::new() // Empty means all tracks
            };

            // Create output directory if it doesn't exist
            if !output.exists() {
                fs::create_dir_all(&output).context("Failed to create output directory")?;
            }

            // Build disc prefix for filenames
            let disc_title = sb_reader
                .get_master_text()
                .and_then(|mt| mt.disc_title.as_ref())
                .map(|s| s.as_str());
            let disc_artist = sb_reader
                .get_master_text()
                .and_then(|mt| mt.disc_artist.as_ref())
                .map(|s| s.as_str());
            let disc_prefix = scarletbook::extractor::build_disc_prefix(disc_title, disc_artist);

            println!("\n=== Starting Extraction ===");
            println!("Output directory: {}", output.display());

            // Get TOCs before consuming sb_reader
            let stereo_toc = sb_reader.get_stereo_toc();
            let mch_toc = sb_reader.get_mch_toc();
            let master_toc = sb_reader.get_master_toc();
            let master_text = sb_reader.get_master_text().cloned();

            // Get reader from sb_reader and create extractor
            let reader = sb_reader.into_reader();
            let mut extractor = scarletbook::extractor::TrackExtractor::new(reader);

            // Extract stereo tracks
            if extract_stereo {
                if let Some(stereo_toc) = stereo_toc {
                    println!("\n--- Extracting Stereo Tracks ---");
                    extractor
                        .extract_tracks(
                            &master_toc,
                            master_text.as_ref(),
                            &stereo_toc,
                            &selected_tracks,
                            &output,
                            &format!("{}-stereo", disc_prefix),
                        )
                        .context("Failed to extract stereo tracks")?;
                } else {
                    println!("\nNo stereo tracks found on disc");
                }
            }

            // Extract multi-channel tracks
            if extract_mch {
                if let Some(mch_toc) = mch_toc {
                    println!("\n--- Extracting Multi-Channel Tracks ---");
                    extractor
                        .extract_tracks(
                            &master_toc,
                            master_text.as_ref(),
                            &mch_toc,
                            &selected_tracks,
                            &output,
                            &format!("{}-mch", disc_prefix),
                        )
                        .context("Failed to extract multi-channel tracks")?;
                } else {
                    println!("\nNo multi-channel tracks found on disc");
                }
            }

            println!("\n=== Extraction Complete ===");

            Ok(())
        }
        Commands::ExtractNet {
            server,
            output,
            stereo,
            multichannel: multi_channel,
            tracks,
        } => {
            let (ip, port) = parse_server_address(&server)?;

            // If neither flag is specified, extract both
            let extract_stereo = if !stereo && !multi_channel {
                true // Default: extract both
            } else {
                stereo
            };
            let extract_mch = if !stereo && !multi_channel {
                true // Default: extract both
            } else {
                multi_channel
            };

            println!("Connecting to {}:{}...", ip, port);
            let net_reader = NetReader::open_network_reader(ip, port)
                .context("Failed to connect to SACD server")?;
            println!("Connected!");

            println!("Reading disc metadata...");
            let mut sb_reader = scarletbook::reader::new(net_reader)
                .context("Failed to read SACD metadata from network")?;

            // Print disc info
            sb_reader.print_disc_info();

            // Parse track selection
            let selected_tracks = if let Some(track_str) = tracks {
                parse_track_selection(&track_str)?
            } else {
                Vec::new() // Empty means all tracks
            };

            // Create output directory if it doesn't exist
            if !output.exists() {
                fs::create_dir_all(&output).context("Failed to create output directory")?;
            }

            // Build disc prefix for filenames
            let disc_title = sb_reader
                .get_master_text()
                .and_then(|mt| mt.disc_title.as_ref())
                .map(|s| s.as_str());
            let disc_artist = sb_reader
                .get_master_text()
                .and_then(|mt| mt.disc_artist.as_ref())
                .map(|s| s.as_str());
            let disc_prefix = scarletbook::extractor::build_disc_prefix(disc_title, disc_artist);

            println!("\n=== Starting Extraction ===");
            println!("Output directory: {}", output.display());

            // Get TOCs before consuming sb_reader
            let stereo_toc = sb_reader.get_stereo_toc();
            let mch_toc = sb_reader.get_mch_toc();
            let master_toc = sb_reader.get_master_toc();
            let master_text = sb_reader.get_master_text().cloned();

            // Get reader from sb_reader and create extractor
            let reader = sb_reader.into_reader();
            let mut extractor = scarletbook::extractor::TrackExtractor::new(reader);

            // Extract stereo tracks
            if extract_stereo {
                if let Some(stereo_toc) = stereo_toc {
                    println!("\n--- Extracting Stereo Tracks ---");
                    extractor
                        .extract_tracks(
                            &master_toc,
                            master_text.as_ref(),
                            &stereo_toc,
                            &selected_tracks,
                            &output,
                            &format!("{}-stereo", disc_prefix),
                        )
                        .context("Failed to extract stereo tracks")?;
                } else {
                    println!("\nNo stereo tracks found on disc");
                }
            }

            // Extract multi-channel tracks
            if extract_mch {
                if let Some(mch_toc) = mch_toc {
                    println!("\n--- Extracting Multi-Channel Tracks ---");
                    extractor
                        .extract_tracks(
                            &master_toc,
                            master_text.as_ref(),
                            &mch_toc,
                            &selected_tracks,
                            &output,
                            &format!("{}-mch", disc_prefix),
                        )
                        .context("Failed to extract multi-channel tracks")?;
                } else {
                    println!("\nNo multi-channel tracks found on disc");
                }
            }

            println!("\n=== Extraction Complete ===");

            Ok(())
        }
    }
}

/// Parse track selection string (e.g., "1,2,5" or "1-3,5")
fn parse_track_selection(tracks: &str) -> Result<Vec<usize>> {
    let mut selected = Vec::new();

    for part in tracks.split(',') {
        let part = part.trim();
        if part.contains('-') {
            // Range like "1-3"
            let range_parts: Vec<&str> = part.split('-').collect();
            if range_parts.len() != 2 {
                anyhow::bail!("Invalid track range: {}", part);
            }
            let start: usize = range_parts[0]
                .parse()
                .with_context(|| format!("Invalid track number: {}", range_parts[0]))?;
            let end: usize = range_parts[1]
                .parse()
                .with_context(|| format!("Invalid track number: {}", range_parts[1]))?;

            if start == 0 || end == 0 {
                anyhow::bail!("Track numbers must be 1 or greater");
            }
            if start > end {
                anyhow::bail!("Invalid range: {} > {}", start, end);
            }

            for track in start..=end {
                if !selected.contains(&track) {
                    selected.push(track);
                }
            }
        } else {
            // Single track
            let track: usize = part
                .parse()
                .with_context(|| format!("Invalid track number: {}", part))?;
            if track == 0 {
                anyhow::bail!("Track numbers must be 1 or greater");
            }
            if !selected.contains(&track) {
                selected.push(track);
            }
        }
    }

    selected.sort_unstable();
    Ok(selected)
}
