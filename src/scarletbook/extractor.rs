use crate::sacd_reader::SacdReader;
use crate::scarletbook::area_toc::AreaToc;
use crate::scarletbook::audio::AudioSectorParser;
use crate::scarletbook::dsf::{DsfWriter, DSD64_SAMPLE_RATE};
use crate::scarletbook::id3::render_id3;
use crate::scarletbook::master_toc::{MasterText, MasterToc};
use crate::scarletbook::types::FrameFormat;
use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use log::debug;

/// Track extractor for extracting SACD tracks to DSF files
pub struct TrackExtractor<R: SacdReader> {
    reader: R,
}

impl<R: SacdReader> TrackExtractor<R> {
    /// Create a new track extractor
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    /// Extract a single track to a DSF file
    pub fn extract_track(
        &mut self,
        master_toc: &MasterToc,
        master_text: Option<&MasterText>,
        area_toc: &AreaToc,
        track_number: usize,
        output_path: &Path,
        progress_bar: Option<&ProgressBar>,
    ) -> Result<()> {
        // Validate track number
        if track_number < 1 || track_number > area_toc.track_count as usize {
            anyhow::bail!(
                "Invalid track number: {} (valid range: 1-{})",
                track_number,
                area_toc.track_count
            );
        }

        // Get track info
        let track_idx = track_number - 1; // Convert to 0-based
        let track_start_time = area_toc.track_times_start.get(track_idx).ok_or_else(|| {
            anyhow::anyhow!("Track {} start time not found", track_number)
        })?;
        let track_duration = area_toc
            .track_times_duration
            .get(track_idx)
            .ok_or_else(|| anyhow::anyhow!("Track {} duration not found", track_number))?;

        // Calculate start and end LSNs for the track
        // SACD timing: 75 frames per second, each frame is 588 sectors
        let start_frame = track_start_time.minutes as u32 * 60 * 75
            + track_start_time.seconds as u32 * 75
            + track_start_time.frames as u32;
        let duration_frames = track_duration.minutes as u32 * 60 * 75
            + track_duration.seconds as u32 * 75
            + track_duration.frames as u32;

        // Get sectors per frame
        let sectors_per_frame = area_toc
            .frame_format
            .sectors_per_frame()
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Unsupported or unknown frame format: {:?}",
                    area_toc.frame_format
                )
            })?;

        // Prefer explicit per-track LSN ranges from SACDTRL1; fall back to
        // estimating from timecodes only if the disc didn't include them.
        let (start_lsn, end_lsn) = match (
            area_toc.track_start_lsns.get(track_idx).copied(),
            area_toc.track_length_lsns.get(track_idx).copied(),
        ) {
            (Some(start), Some(length)) if start != 0 && length != 0 => {
                (start, start + length)
            }
            _ => {
                let start_lsn = if track_idx == 0 {
                    area_toc.track_start
                } else {
                    area_toc.track_start + (start_frame * sectors_per_frame / 3)
                };
                let end_lsn = if track_idx < area_toc.track_count as usize - 1 {
                    let next = area_toc.track_times_start.get(track_idx + 1)
                        .ok_or_else(|| anyhow::anyhow!("Next track start time not found"))?;
                    let next_fc = next.minutes as u32 * 60 * 75
                        + next.seconds as u32 * 75
                        + next.frames as u32;
                    area_toc.track_start + (next_fc * sectors_per_frame / 3)
                } else {
                    area_toc.track_end
                };
                (start_lsn, end_lsn)
            }
        };

        let total_sectors = end_lsn - start_lsn;

        log::info!(
            "Track {}: start_lsn={}, end_lsn={}, total_sectors={}, area.track_start={}, area.track_end={}, channels={}",
            track_number, start_lsn, end_lsn, total_sectors, area_toc.track_start, area_toc.track_end, area_toc.channel_count
        );

        debug!("[EXTRACTION] Track {}: channels={}, LSN range: {}-{}, frame_format={:?}",
                  track_number, area_toc.channel_count, start_lsn, end_lsn, area_toc.frame_format);

        if let Some(pb) = progress_bar {
            pb.set_length(total_sectors as u64);
            pb.set_message(format!("Track {}", track_number));
        }

        // Calculate total samples per channel for DSF header
        // DSD64: 2822400 Hz, so samples = duration_seconds * 2822400
        let duration_seconds = track_duration.minutes as u64 * 60
            + track_duration.seconds as u64
            + track_duration.frames as u64 / 75;
        let total_samples_per_channel = duration_seconds * DSD64_SAMPLE_RATE as u64;

        // Create DSF writer
        let mut dsf_writer = DsfWriter::create(
            output_path,
            area_toc.channel_count as u32,
            DSD64_SAMPLE_RATE,
            total_samples_per_channel,
            area_toc.extra_setting,
        )
        .context("Failed to create DSF file")?;

        // Generate ID3v2.3 footer matching the layout sacd-ripper writes.
        let id3 = render_id3(master_toc, master_text, area_toc, track_idx);
        dsf_writer.set_id3_footer(id3);

        // Create audio sector parser
        let mut audio_parser = AudioSectorParser::new(
            area_toc.frame_format,
            area_toc.channel_count as usize,
            DSD64_SAMPLE_RATE as usize,
        )?;

        // Timecode filter — only frames whose timecode is in
        // [track_start, track_start + track_duration) are decoded. This
        // matches sacd-ripper's default `audio_frame_trimming=1` mode.
        let start_fc = track_start_time.minutes as u32 * 60 * 75
            + track_start_time.seconds as u32 * 75
            + track_start_time.frames as u32;
        let dur_fc = track_duration.minutes as u32 * 60 * 75
            + track_duration.seconds as u32 * 75
            + track_duration.frames as u32;
        let end_fc = start_fc + dur_fc;
        // The existing helper takes m/s/f triples, so convert.
        audio_parser.set_timecode_filter(
            (start_fc / (60 * 75)) as u8,
            ((start_fc / 75) % 60) as u8,
            (start_fc % 75) as u8,
            (end_fc / (60 * 75)) as u8,
            ((end_fc / 75) % 60) as u8,
            (end_fc % 75) as u8,
        );

        // Read and process sectors in batches to amortise per-call overhead
        // (per-sector seek+read syscalls dominated wall-time before this).
        const BATCH_SECTORS: u32 = 256;
        const SECTOR: usize = 2048;
        let mut current_lsn = start_lsn;
        let mut sectors_processed = 0u32;

        while current_lsn < end_lsn {
            let want = (end_lsn - current_lsn).min(BATCH_SECTORS);
            let chunk = self
                .reader
                .read_data(current_lsn, want)
                .with_context(|| format!("Failed to read {} sectors at LSN {}", want, current_lsn))?;
            if chunk.len() < (want as usize) * SECTOR {
                anyhow::bail!(
                    "Short read at LSN {}: got {} bytes, expected {}",
                    current_lsn,
                    chunk.len(),
                    (want as usize) * SECTOR
                );
            }
            for i in 0..want {
                let off = (i as usize) * SECTOR;
                let sector = &chunk[off..off + SECTOR];
                if let Some(frame_data) = audio_parser.parse_sector(sector)? {
                    dsf_writer.write_samples(&frame_data)?;
                }
            }
            current_lsn += want;
            sectors_processed += want;
            if let Some(pb) = progress_bar {
                pb.set_position(sectors_processed as u64);
            }
        }

        // Flush any remaining data
        if let Some(frame_data) = audio_parser.flush() {
            dsf_writer.write_samples(&frame_data)?;
        }

        log::info!(
            "Track {}: decoded {} frames ({} filtered), {} sectors walked",
            track_number,
            audio_parser.decoded_frames,
            audio_parser.filtered_frames,
            sectors_processed,
        );

        // Finalize DSF file
        dsf_writer.finalize()?;

        if let Some(pb) = progress_bar {
            pb.finish_with_message(format!("Track {} complete", track_number));
        }

        Ok(())
    }

    /// Extract multiple tracks from an area
    ///
    /// # Arguments
    /// * `area_toc` - Area TOC containing track information
    /// * `track_numbers` - List of track numbers to extract (1-based). If empty, extract all tracks.
    /// * `output_dir` - Output directory for DSF files
    /// * `prefix` - Filename prefix (e.g., "disc_title")
    pub fn extract_tracks(
        &mut self,
        master_toc: &MasterToc,
        master_text: Option<&MasterText>,
        area_toc: &AreaToc,
        track_numbers: &[usize],
        output_dir: &Path,
        prefix: &str,
    ) -> Result<()> {
        // Determine which tracks to extract
        let tracks_to_extract: Vec<usize> = if track_numbers.is_empty() {
            // Extract all tracks
            (1..=area_toc.track_count as usize).collect()
        } else {
            track_numbers.to_vec()
        };

        println!(
            "Extracting {} tracks from {} area...",
            tracks_to_extract.len(),
            if area_toc.channel_count == 2 {
                "stereo"
            } else {
                "multi-channel"
            }
        );

        // Extract each track
        for &track_num in &tracks_to_extract {
            // Get track title if available
            let track_title = area_toc
                .track_texts
                .get(track_num - 1)
                .and_then(|tt| tt.title.as_ref())
                .map(|s| sanitize_filename(s))
                .unwrap_or_else(|| format!("Track_{:02}", track_num));

            // Build output filename
            let filename = format!("{}-{:02}-{}.dsf", prefix, track_num, track_title);
            let output_path = output_dir.join(filename);

            println!("\nExtracting track {}: {}", track_num, track_title);

            // Create progress bar
            let pb = ProgressBar::new(0);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} sectors ({percent}%) [{elapsed_precise}]")
                    .unwrap()
                    .progress_chars("#>-"),
            );

            // Extract track
            self.extract_track(
                master_toc,
                master_text,
                area_toc,
                track_num,
                &output_path,
                Some(&pb),
            )?;

            println!("Saved to: {}", output_path.display());
        }

        Ok(())
    }

    /// Get mutable reference to the underlying reader
    pub fn reader_mut(&mut self) -> &mut R {
        &mut self.reader
    }
}

/// Sanitize a filename by removing invalid characters
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

/// Helper function to build a disc prefix for filenames
pub fn build_disc_prefix(disc_title: Option<&str>, disc_artist: Option<&str>) -> String {
    match (disc_title, disc_artist) {
        (Some(title), Some(artist)) => {
            format!("{}-{}", sanitize_filename(artist), sanitize_filename(title))
        }
        (Some(title), None) => sanitize_filename(title),
        (None, Some(artist)) => sanitize_filename(artist),
        (None, None) => "Unknown_Disc".to_string(),
    }
}
