use crate::dst_decoder::decoder::DstDecoder;
use crate::sacd_reader::SacdReader;
use crate::scarletbook::area_toc::AreaToc;
use crate::scarletbook::audio::AudioSectorParser;
use crate::scarletbook::consts::{DSD64_SAMPLE_RATE, FRAMES_PER_MINUTE, FRAMES_PER_SECOND};
use crate::scarletbook::dsf::DsfWriter;
use crate::scarletbook::id3::render_id3;
use crate::scarletbook::master_toc::{MasterText, MasterToc};
use crate::scarletbook::types::FrameFormat;
use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use log::debug;
use rayon::prelude::*;
use std::cell::RefCell;
use std::path::Path;

/// Track extractor for extracting SACD tracks to DSF files
pub struct TrackExtractor<R: SacdReader> {
    reader: R,
}

impl<R: SacdReader> TrackExtractor<R> {
    /// Create a new track extractor
    pub fn new(reader: R) -> Self {
        Self { reader }
    }

    /// Extract a single track to a DSF file. The reader must be `Send`
    /// because the producer phase of the streaming pipeline runs on a
    /// separate thread (sectors are read in parallel with decode/write).
    pub fn extract_track(
        &mut self,
        master_toc: &MasterToc,
        master_text: Option<&MasterText>,
        area_toc: &AreaToc,
        track_number: usize,
        output_path: &Path,
        progress_bar: Option<&ProgressBar>,
    ) -> Result<()>
    where
        R: Send,
    {
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
        let track_start_time = area_toc
            .track_times_start
            .get(track_idx)
            .ok_or_else(|| anyhow::anyhow!("Track {} start time not found", track_number))?;
        let track_duration = area_toc
            .track_times_duration
            .get(track_idx)
            .ok_or_else(|| anyhow::anyhow!("Track {} duration not found", track_number))?;

        // Calculate start frame for the track.
        let start_frame = track_start_time.minutes as u32 * FRAMES_PER_MINUTE
            + track_start_time.seconds as u32 * FRAMES_PER_SECOND
            + track_start_time.frames as u32;

        // Get sectors per frame
        let sectors_per_frame = area_toc.frame_format.sectors_per_frame().ok_or_else(|| {
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
            (Some(start), Some(length)) if start != 0 && length != 0 => (start, start + length),
            _ => {
                let start_lsn = if track_idx == 0 {
                    area_toc.track_start
                } else {
                    area_toc.track_start + (start_frame * sectors_per_frame / 3)
                };
                let end_lsn = if track_idx < area_toc.track_count as usize - 1 {
                    let next = area_toc
                        .track_times_start
                        .get(track_idx + 1)
                        .ok_or_else(|| anyhow::anyhow!("Next track start time not found"))?;
                    let next_fc = next.minutes as u32 * FRAMES_PER_MINUTE
                        + next.seconds as u32 * FRAMES_PER_SECOND
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
            track_number,
            start_lsn,
            end_lsn,
            total_sectors,
            area_toc.track_start,
            area_toc.track_end,
            area_toc.channel_count
        );

        debug!(
            "[EXTRACTION] Track {}: channels={}, LSN range: {}-{}, frame_format={:?}",
            track_number, area_toc.channel_count, start_lsn, end_lsn, area_toc.frame_format
        );

        // Drive the progress bar by output frames written.
        // Total expected frames == track duration in 1/75-second frames
        // (already computed as `dur_fc` below).
        if let Some(pb) = progress_bar {
            let total_frames = track_duration.minutes as u64 * FRAMES_PER_MINUTE as u64
                + track_duration.seconds as u64 * FRAMES_PER_SECOND as u64
                + track_duration.frames as u64;
            pb.set_length(total_frames);
            pb.set_message(format!("Track {}", track_number));
        }

        // Total samples per channel for DSF header.
        let duration_seconds = track_duration.minutes as u64 * 60
            + track_duration.seconds as u64
            + track_duration.frames as u64 / FRAMES_PER_SECOND as u64;
        let total_samples_per_channel = duration_seconds * DSD64_SAMPLE_RATE as u64;

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

        // Create audio sector parser; tell it the area's channel count so
        // it can cross-check each frame's per-frame `channel_bits` hint.
        let mut audio_parser = AudioSectorParser::new(area_toc.frame_format)?;
        audio_parser.set_expected_channel_count(area_toc.channel_count);

        // Timecode filter — only frames whose timecode is in
        // [track_start, track_start + track_duration) are decoded. This
        // matches sacd-ripper's default `audio_frame_trimming=1` mode.
        let start_fc = track_start_time.minutes as u32 * FRAMES_PER_MINUTE
            + track_start_time.seconds as u32 * FRAMES_PER_SECOND
            + track_start_time.frames as u32;
        let dur_fc = track_duration.minutes as u32 * FRAMES_PER_MINUTE
            + track_duration.seconds as u32 * FRAMES_PER_SECOND
            + track_duration.frames as u32;
        let end_fc = start_fc + dur_fc;
        // The existing helper takes m/s/f triples, so convert.
        audio_parser.set_timecode_filter(
            (start_fc / FRAMES_PER_MINUTE) as u8,
            ((start_fc / FRAMES_PER_SECOND) % 60) as u8,
            (start_fc % FRAMES_PER_SECOND) as u8,
            (end_fc / FRAMES_PER_MINUTE) as u8,
            ((end_fc / FRAMES_PER_SECOND) % 60) as u8,
            (end_fc % FRAMES_PER_SECOND) as u8,
        );

        // Streaming producer-consumer pipeline. A producer thread reads
        // sectors and parses them into raw frame bytes, sending each frame
        // through a bounded mpsc channel. The main thread receives frames in
        // batches, decodes each batch in parallel (rayon, thread-local
        // DstDecoders), and writes the decoded frames to the DSF in order.
        //
        // The bounded channel is what enables I/O–compute overlap for slow
        // sources (e.g. NetReader): the producer keeps fetching sectors
        // while the consumer is busy decoding/writing. For a fast local ISO
        // the producer simply backpressures on a full channel; total work
        // is the same as a sequential decode.
        //
        // Memory cap: CHANNEL_CAPACITY raw DST frames in flight + one
        // BATCH_SIZE-sized batch being decoded ≈ <10 MB for DSD64.
        const BATCH_SECTORS: u32 = 256;
        const SECTOR: usize = 2048;
        const BATCH_SIZE: usize = 256;
        // ~16k raw DST frames in flight ≈ ~50 MB at typical 3 KB/frame.
        // Generous enough to absorb multi-second network stalls; for a
        // local ISO the producer just backpressures on a full channel
        // before this fills up.
        const CHANNEL_CAPACITY: usize = 16384;

        let is_dst = matches!(area_toc.frame_format, FrameFormat::Dst);
        let channels = area_toc.channel_count as usize;
        let sample_rate = DSD64_SAMPLE_RATE as usize;
        let track_num = track_number;

        let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(CHANNEL_CAPACITY);
        let reader: &mut R = &mut self.reader;

        std::thread::scope(|s| -> Result<()> {
            // ---- Producer ----
            let producer = s.spawn(move || -> Result<(u64, u64)> {
                let mut current_lsn = start_lsn;
                'outer: while current_lsn < end_lsn {
                    let want = (end_lsn - current_lsn).min(BATCH_SECTORS);
                    let chunk = reader
                        .read_data(current_lsn, want)
                        .with_context(|| format!("read {} sectors @ LSN {}", want, current_lsn))?;
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
                        if let Some(raw) = audio_parser.parse_sector(sector)?
                            && tx.send(raw).is_err()
                        {
                            // Consumer dropped (almost certainly due to
                            // a write/decode error). Stop producing —
                            // the consumer's error will be propagated.
                            break 'outer;
                        }
                    }
                    current_lsn += want;
                }
                if let Some(raw) = audio_parser.flush() {
                    let _ = tx.send(raw);
                }
                Ok((audio_parser.decoded_frames, audio_parser.filtered_frames))
                // tx dropped here -> consumer's recv() returns Err, ending its loop.
            });

            // ---- Consumer (this thread) ----
            // Run the consumer in an inner scope so `rx` is dropped when it
            // returns (including via `?`). Dropping `rx` unblocks any
            // producer waiting on a full channel after a consumer error.
            let consumer_pb = progress_bar.cloned();
            // Per-track rayon worker pool. Building a dedicated pool here
            // (instead of using rayon's global pool) means the worker
            // threads — and therefore their `TLS_DECODER` slots — are
            // created fresh for each track, which simplifies init.
            let pool = rayon::ThreadPoolBuilder::new()
                .thread_name(|i| format!("dst-decode-{}", i))
                .build()
                .map_err(|e| anyhow::anyhow!("rayon pool: {}", e))?;

            let consumer_res = (|rx: std::sync::mpsc::Receiver<Vec<u8>>| -> Result<()> {
                thread_local! {
                    static TLS_DECODER: RefCell<Option<DstDecoder>> = const {
                        RefCell::new(None)
                    };
                }
                let mut batch: Vec<Vec<u8>> = Vec::with_capacity(BATCH_SIZE);
                let mut frames_written: u64 = 0;
                let mut process = |batch: &mut Vec<Vec<u8>>,
                                   dsf_writer: &mut DsfWriter|
                 -> Result<()> {
                    if batch.is_empty() {
                        return Ok(());
                    }
                    let n = batch.len() as u64;
                    if is_dst {
                        // Parallel decode of this batch on the
                        // per-track pool.
                        let decoded: Result<Vec<Vec<u8>>> = pool.install(|| {
                            batch
                                .par_iter()
                                .map(|raw| -> Result<Vec<u8>> {
                                    TLS_DECODER.with(|cell| -> Result<Vec<u8>> {
                                        let mut slot = cell.borrow_mut();
                                        if slot.is_none() {
                                            *slot = Some(DstDecoder::new(channels, sample_rate)?);
                                        }
                                        let dec = slot.as_mut().unwrap();
                                        let mut buf = vec![0u8; dec.dsd_frame_bytes()];
                                        let n = dec.decode_frame(raw, &mut buf)?;
                                        buf.truncate(n);
                                        Ok(buf)
                                    })
                                })
                                .collect()
                        });
                        for f in decoded? {
                            dsf_writer.write_samples(&f)?;
                        }
                    } else {
                        for raw in batch.iter() {
                            dsf_writer.write_samples(raw)?;
                        }
                    }
                    batch.clear();
                    frames_written += n;
                    if let Some(pb) = consumer_pb.as_ref() {
                        pb.set_position(frames_written);
                    }
                    Ok(())
                };

                for raw in rx.iter() {
                    batch.push(raw);
                    if batch.len() >= BATCH_SIZE {
                        process(&mut batch, &mut dsf_writer)?;
                    }
                }
                process(&mut batch, &mut dsf_writer)?;
                Ok(())
            })(rx);

            let producer_res = producer
                .join()
                .map_err(|_| anyhow::anyhow!("producer thread panicked"))?;

            // Surface whichever side failed first.
            consumer_res?;
            let (decoded_n, filtered_n) = producer_res?;
            log::info!(
                "Track {}: yielded {} frames ({} filtered)",
                track_num,
                decoded_n,
                filtered_n,
            );
            Ok(())
        })?;

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
    ) -> Result<()>
    where
        R: Send,
    {
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
                    .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} frames ({percent}%) [{elapsed_precise}]")
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
