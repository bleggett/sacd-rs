use crate::scarletbook::types::FrameFormat;
use crate::dst_decoder::decoder::DstDecoder;
use anyhow::Result;
use log::{trace, debug};

/// SACD audio frame header (first byte of each audio sector)
#[derive(Debug, Clone, Copy)]
pub struct AudioFrameHeader {
    /// DST encoded flag
    pub dst_encoded: bool,
    /// Number of frame starts (N_Frame_Starts)
    pub frame_info_count: u8,
    /// Number of packets (N_Packets)
    pub packet_info_count: u8,
}

impl AudioFrameHeader {
    /// Parse audio frame header from a byte
    pub fn from_byte(byte: u8) -> Self {
        Self {
            dst_encoded: (byte & 0x01) != 0,
            frame_info_count: (byte >> 2) & 0x07,
            packet_info_count: (byte >> 5) & 0x07,
        }
    }
}

/// SACD audio packet info
#[derive(Debug, Clone, Copy)]
pub struct AudioPacketInfo {
    /// Frame start flag
    pub frame_start: bool,
    /// Data type
    pub data_type: u8,
    /// Packet length in bytes
    pub packet_length: u16,
}

impl AudioPacketInfo {
    /// Parse audio packet info from 2 bytes
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            frame_start: (bytes[0] & 0x80) != 0,
            data_type: (bytes[0] >> 3) & 0x07,
            packet_length: ((bytes[0] as u16 & 0x07) << 8) | bytes[1] as u16,
        }
    }
}

/// SACD audio frame info (from sector headers)
#[derive(Debug, Clone, Copy)]
pub struct AudioFrameInfo {
    /// Timecode
    pub minutes: u8,
    pub seconds: u8,
    pub frames: u8,
    /// Sector count (DST frames only) - number of sectors remaining for this frame
    pub sector_count: u8,
    /// Channel info
    pub channel_bits: u8,
}

impl AudioFrameInfo {
    /// Parse frame info from bytes
    pub fn from_bytes(bytes: &[u8], dst_encoded: bool) -> Self {
        let size = if dst_encoded { 4 } else { 3 };
        assert!(bytes.len() >= size, "Frame info too short");

        Self {
            minutes: bytes[0],
            seconds: bytes[1],
            frames: bytes[2],
            // For little-endian: channel_bit_3 (bit 0), channel_bit_2 (bit 1), sector_count (bits 2-6), channel_bit_1 (bit 7)
            sector_count: if dst_encoded { (bytes[3] >> 2) & 0x1F } else { 0 },
            channel_bits: if dst_encoded { bytes[3] & 0xE1 } else { 0 },
        }
    }

    /// Convert timecode to frame count (matching C code's TIME_FRAMECOUNT macro)
    /// Formula: minutes * 60 * 75 + seconds * 75 + frames
    pub fn to_frame_count(&self) -> u32 {
        self.minutes as u32 * 60 * 75 + self.seconds as u32 * 75 + self.frames as u32
    }
}

/// Audio data types
const DATA_TYPE_AUDIO: u8 = 2;
const DATA_TYPE_SUPPLEMENTARY: u8 = 3;
const DATA_TYPE_PADDING: u8 = 7;

/// Audio sector parser for extracting DSD samples from SACD audio sectors
pub struct AudioSectorParser {
    /// Frame format (DST, DSD 3-in-14, DSD 3-in-16, etc.)
    frame_format: FrameFormat,
    /// Accumulated audio data for the current frame
    frame_buffer: Vec<u8>,
    /// Total bytes extracted
    total_bytes: u64,
    /// DST decoder (only used for DST-compressed audio)
    dst_decoder: Option<DstDecoder>,
    /// Decode counters (debug aid).
    pub decoded_frames: u64,
    pub filtered_frames: u64,
    /// Remaining sector count for current DST frame
    dst_sector_count: i32,
    /// Frame has started
    frame_started: bool,
    /// Optional timecode filtering (start frame count, end frame count)
    /// When set, only frames with timecodes in [start, end) range are extracted
    timecode_filter: Option<(u32, u32)>,
    /// Current frame's timecode (as frame count) for filtering
    current_frame_timecode: Option<u32>,
}

impl AudioSectorParser {
    /// Create a new audio sector parser
    ///
    /// # Arguments
    /// * `frame_format` - The frame format (DST, DSD, etc.)
    /// * `channel_count` - Number of audio channels
    /// * `sample_rate` - Sample rate in Hz
    pub fn new(frame_format: FrameFormat, channel_count: usize, sample_rate: usize) -> Result<Self> {
        // Initialize DST decoder if needed
        let dst_decoder = if matches!(frame_format, FrameFormat::Dst) {
            Some(DstDecoder::new(channel_count, sample_rate)?)
        } else {
            None
        };

        Ok(Self {
            frame_format,
            frame_buffer: Vec::new(),
            total_bytes: 0,
            dst_decoder,
            dst_sector_count: 0,
            frame_started: false,
            timecode_filter: None,
            current_frame_timecode: None,
            decoded_frames: 0,
            filtered_frames: 0,
        })
    }

    /// Set timecode filter to only extract frames within a specific time range
    ///
    /// # Arguments
    /// * `start_minutes`, `start_seconds`, `start_frames` - Start timecode (inclusive)
    /// * `end_minutes`, `end_seconds`, `end_frames` - End timecode (exclusive)
    ///
    /// When set, only frames with timecodes >= start and < end will be extracted.
    /// This matches the C code's audio_frame_trimming behavior.
    pub fn set_timecode_filter(
        &mut self,
        start_minutes: u8,
        start_seconds: u8,
        start_frames: u8,
        end_minutes: u8,
        end_seconds: u8,
        end_frames: u8,
    ) {
        let start_frame_count = start_minutes as u32 * 60 * 75 + start_seconds as u32 * 75 + start_frames as u32;
        let end_frame_count = end_minutes as u32 * 60 * 75 + end_seconds as u32 * 75 + end_frames as u32;
        self.timecode_filter = Some((start_frame_count, end_frame_count));
        log::info!(
            "Timecode filter set: [{:02}:{:02}:{:02} - {:02}:{:02}:{:02}) = [frame {} - frame {})",
            start_minutes, start_seconds, start_frames,
            end_minutes, end_seconds, end_frames,
            start_frame_count, end_frame_count
        );
    }

    /// Parse an audio sector and extract DSD samples
    ///
    /// Returns the extracted audio data if a complete frame is ready, otherwise None.
    ///
    /// # Arguments
    /// * `sector_data` - Raw sector data (2048 bytes)
    pub fn parse_sector(&mut self, sector_data: &[u8]) -> Result<Option<Vec<u8>>> {
        if sector_data.len() < 2048 {
            anyhow::bail!(
                "Audio sector too short: {} bytes (expected 2048)",
                sector_data.len()
            );
        }

        // Debug: log first sector
        static SECTOR_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let sector_num = SECTOR_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if sector_num < 3 {
            debug!("[SECTOR #{}] First 16 bytes: {:02x?}", sector_num, &sector_data[..16]);
        }

        // Parse audio frame header (first byte)
        let header = AudioFrameHeader::from_byte(sector_data[0]);
        let mut offset = 1;

        log::debug!(
            "Audio sector header: dst_encoded={}, frame_info_count={}, packet_info_count={}, frame_format={:?}",
            header.dst_encoded,
            header.frame_info_count,
            header.packet_info_count,
            self.frame_format
        );

        // Parse packet info headers
        let mut packets = Vec::new();
        for i in 0..header.packet_info_count {
            if offset + 2 > sector_data.len() {
                anyhow::bail!("Invalid audio sector: packet info extends beyond sector");
            }
            let packet_info = AudioPacketInfo::from_bytes(&sector_data[offset..offset + 2]);

            // Debug: log first few packets with raw bytes
            if sector_num < 3 && i < 3 {
                debug!("[PKT_INFO sector={} pkt={}] raw_bytes=[{:02x}, {:02x}], frame_start={}, data_type={}, len={}",
                         sector_num, i, sector_data[offset], sector_data[offset+1],
                         packet_info.frame_start, packet_info.data_type, packet_info.packet_length);
            }

            packets.push(packet_info);
            offset += 2;
        }

        // Parse frame info headers
        let frame_info_size = if header.dst_encoded { 4 } else { 3 };
        let mut frame_infos = Vec::new();
        for i in 0..header.frame_info_count {
            if offset + frame_info_size > sector_data.len() {
                anyhow::bail!("Invalid audio sector: frame info extends beyond sector");
            }
            let frame_info = AudioFrameInfo::from_bytes(
                &sector_data[offset..offset + frame_info_size],
                header.dst_encoded
            );
            log::debug!(
                "Frame info [{}]: timecode={}:{}:{}, sector_count={}",
                i,
                frame_info.minutes,
                frame_info.seconds,
                frame_info.frames,
                frame_info.sector_count
            );
            frame_infos.push(frame_info);
            offset += frame_info_size;
        }

        if offset > sector_data.len() {
            anyhow::bail!(
                "Invalid audio sector: calculated header size {} exceeds sector size {}",
                offset,
                sector_data.len()
            );
        }

        // Process each packet and extract audio data
        let mut frame_info_idx = 0;
        for packet in packets {
            if offset + packet.packet_length as usize > sector_data.len() {
                anyhow::bail!(
                    "Invalid audio sector: packet at offset {} with length {} extends beyond sector",
                    offset,
                    packet.packet_length
                );
            }

            // Only extract audio packets, skip padding and supplementary
            if packet.data_type == DATA_TYPE_AUDIO {
                let packet_data = &sector_data[offset..offset + packet.packet_length as usize];

                static PKT_ALL_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                let pkt_all = PKT_ALL_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if pkt_all < 15 {
                    debug!("[AUDIO_PKT #{}] frame_start={}, len={}, frame_started={}",
                             pkt_all, packet.frame_start, packet_data.len(), self.frame_started);
                }

                // Debug: log first packet of first few frames
                if self.frame_started && self.frame_buffer.len() < 20 && packet_data.len() >= 8 {
                    debug!("[PACKET] Adding packet: offset={}, len={}, first_8_bytes={:02x?}, frame_buf_len_before={}",
                             offset, packet_data.len(), &packet_data[..8.min(packet_data.len())], self.frame_buffer.len());
                }

                trace!(
                    "Audio packet: frame_start={}, length={}, first 8 bytes: {:02x?}",
                    packet.frame_start,
                    packet.packet_length,
                    &packet_data[..8.min(packet_data.len())]
                );

                // Handle DST-compressed audio
                if matches!(self.frame_format, FrameFormat::Dst) {
                    // If this packet marks a new frame start
                    if packet.frame_start {
                        debug!("[FRAME_START_FLAG] packet.frame_start=true, packet_len={}, frame_info_idx={}",
                                 packet_data.len(), frame_info_idx);
                        debug!("Packet has frame_start=true, frame_infos.len()={}, frame_info_idx={}",
                            frame_infos.len(), frame_info_idx);
                        // Get frame info for this frame start
                        if frame_info_idx < frame_infos.len() {
                            let frame_info = &frame_infos[frame_info_idx];
                            frame_info_idx += 1;
                            debug!("[FRAME_INFO] Using frame_info: minutes={}, seconds={}, frames={}, sector_count={}",
                                     frame_info.minutes, frame_info.seconds, frame_info.frames, frame_info.sector_count);

                            debug!("[FRAME_START_CHECK] frame_started={}, dst_sector_count={}, frame_buffer.len()={}",
                                     self.frame_started, self.dst_sector_count, self.frame_buffer.len());

                            // If we have a previous frame that's complete, decode it first
                            if self.frame_started && self.dst_sector_count == 0 && !self.frame_buffer.is_empty() {
                                // Check if previous frame passes timecode filter
                                let should_decode = if let Some((start_fc, end_fc)) = self.timecode_filter {
                                    if let Some(prev_timecode) = self.current_frame_timecode {
                                        let passes = prev_timecode >= start_fc && prev_timecode < end_fc;
                                        log::debug!(
                                            "Timecode filter check: frame {} in range [{}, {})? {}",
                                            prev_timecode, start_fc, end_fc, passes
                                        );
                                        passes
                                    } else {
                                        true // No timecode available, include frame
                                    }
                                } else {
                                    true // No filter set, include all frames
                                };

                                if should_decode {
                                    if let Some(decoder) = &mut self.dst_decoder {
                                        log::info!(
                                            "Decoding complete DST frame at frame_start: {} bytes (timecode={})",
                                            self.frame_buffer.len(),
                                            self.current_frame_timecode.unwrap_or(0)
                                        );

                                        // Debug: Log first bytes of frame being sent to decoder
                                        static DECODE_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                                        let count = DECODE_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                        if count < 5 {
                                            let show_len = self.frame_buffer.len().min(16);
                                            debug!("[DECODE_FRAME #{}] Passing to decoder: len={}, first_16_bytes={:02x?}",
                                                     count, self.frame_buffer.len(), &self.frame_buffer[..show_len]);
                                        }

                                        let mut dsd_buffer = vec![0u8; decoder.dsd_frame_bytes()];
                                        let bytes_decoded = decoder.decode_frame(&self.frame_buffer, &mut dsd_buffer)?;
                                        dsd_buffer.truncate(bytes_decoded);

                                        // Clear and start new frame. The packet that triggered this
                                        // decode is also the *first* packet of the next frame, so
                                        // pre-account for it by decrementing the new sector_count.
                                        // The post-packet decrement at the bottom of the loop is
                                        // skipped due to this early `return`.
                                        self.frame_buffer.clear();
                                        self.frame_buffer.extend_from_slice(packet_data);
                                        self.dst_sector_count = (frame_info.sector_count as i32 - 1).max(0);
                                        self.frame_started = true;
                                        self.current_frame_timecode = Some(frame_info.to_frame_count());

                                        self.total_bytes += bytes_decoded as u64;
                                        self.decoded_frames += 1;
                                        return Ok(Some(dsd_buffer));
                                    } else {
                                        anyhow::bail!("DST decoder not initialized");
                                    }
                                } else {
                                    log::info!(
                                        "Skipping frame outside timecode range: timecode={}",
                                        self.current_frame_timecode.unwrap_or(0)
                                    );
                                    self.filtered_frames += 1;
                                }
                            }

                            // Unconditionally start new frame (matches C code behavior)
                            log::info!("Frame start: resetting frame, new sector_count={}, timecode={:02}:{:02}:{:02}",
                                frame_info.sector_count, frame_info.minutes, frame_info.seconds, frame_info.frames);
                            self.frame_buffer.clear();
                            self.frame_buffer.extend_from_slice(packet_data);

                            // Debug: log first few frame starts
                            static FRAME_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                            let count = FRAME_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            if count < 5 {
                                let show_bytes = packet_data.len().min(16);
                                debug!("[FRAME_START #{} FIRST_PKT] len={}, offset_in_sector={}, first_16_bytes={:02x?}",
                                         count, packet_data.len(), offset, &packet_data[..show_bytes]);
                            }

                            self.dst_sector_count = frame_info.sector_count as i32;
                            self.frame_started = true;
                            self.current_frame_timecode = Some(frame_info.to_frame_count());
                        }
                    } else {
                        // Continue accumulating current frame
                        if self.frame_started {
                            static PKT_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                            let pkt_num = PKT_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            if pkt_num < 20 {
                                debug!("[ACCUM_PKT #{}] Adding {} bytes to frame, new total={}",
                                         pkt_num, packet_data.len(), self.frame_buffer.len() + packet_data.len());
                            }
                            self.frame_buffer.extend_from_slice(packet_data);
                        }
                    }

                    // Decrement sector_count after adding each audio packet (matches C code)
                    if self.frame_started && self.dst_sector_count > 0 {
                        self.dst_sector_count -= 1;
                        log::debug!("After adding packet: buffer_len={}, sector_count={}",
                            self.frame_buffer.len(), self.dst_sector_count);
                    }
                } else {
                    // Uncompressed DSD - just accumulate
                    self.frame_buffer.extend_from_slice(packet_data);
                    self.total_bytes += packet_data.len() as u64;
                }
            }

            offset += packet.packet_length as usize;
        }

        // For uncompressed DSD formats, return data in chunks to avoid building up too much in memory
        // For DST format, check if frame is complete and decode it
        if !matches!(self.frame_format, FrameFormat::Dst) {
            const CHUNK_SIZE: usize = 32768;
            if self.frame_buffer.len() >= CHUNK_SIZE {
                // Return accumulated data as a chunk
                let chunk_data = std::mem::take(&mut self.frame_buffer);
                Ok(Some(chunk_data))
            } else {
                Ok(None)
            }
        } else {
            // DST format - check if frame is complete (sector_count reached 0)
            if self.frame_started && self.dst_sector_count == 0 && !self.frame_buffer.is_empty() {
                // Check if frame passes timecode filter
                let should_decode = if let Some((start_fc, end_fc)) = self.timecode_filter {
                    if let Some(timecode) = self.current_frame_timecode {
                        let passes = timecode >= start_fc && timecode < end_fc;
                        log::debug!(
                            "Timecode filter check (end of sector): frame {} in range [{}, {})? {}",
                            timecode, start_fc, end_fc, passes
                        );
                        passes
                    } else {
                        true // No timecode available, include frame
                    }
                } else {
                    true // No filter set, include all frames
                };

                if should_decode {
                    if let Some(decoder) = &mut self.dst_decoder {
                        log::info!(
                            "Decoding complete DST frame (end of sector): {} bytes, timecode={}, first 16: {:02x?}",
                            self.frame_buffer.len(),
                            self.current_frame_timecode.unwrap_or(0),
                            &self.frame_buffer[..16.min(self.frame_buffer.len())]
                        );

                        // Debug: Log first bytes of frame being sent to decoder
                        static DECODE_COUNT_2: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                        let count2 = DECODE_COUNT_2.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        if count2 < 5 {
                            let show_len = self.frame_buffer.len().min(16);
                            debug!("[DECODE_FRAME_PATH2 #{}] Passing to decoder: len={}, first_16_bytes={:02x?}",
                                     count2, self.frame_buffer.len(), &self.frame_buffer[..show_len]);
                        }

                        let mut dsd_buffer = vec![0u8; decoder.dsd_frame_bytes()];
                        let bytes_decoded = decoder.decode_frame(&self.frame_buffer, &mut dsd_buffer)?;
                        dsd_buffer.truncate(bytes_decoded);

                        // Clear frame state
                        self.frame_buffer.clear();
                        self.frame_started = false;
                        self.dst_sector_count = 0;

                        self.total_bytes += bytes_decoded as u64;
                        self.decoded_frames += 1;
                        return Ok(Some(dsd_buffer));
                    }
                } else {
                    log::info!(
                        "Skipping frame outside timecode range (end of sector): timecode={}",
                        self.current_frame_timecode.unwrap_or(0)
                    );
                    self.filtered_frames += 1;
                    // Clear frame state but don't return data
                    self.frame_buffer.clear();
                    self.frame_started = false;
                    self.dst_sector_count = 0;
                }
            }
            Ok(None)
        }
    }

    /// Flush any remaining buffered data
    ///
    /// Call this at the end of extraction to get any remaining partial frame data.
    pub fn flush(&mut self) -> Option<Vec<u8>> {
        if self.frame_buffer.is_empty() {
            None
        } else if matches!(self.frame_format, FrameFormat::Dst) {
            // Check if frame passes timecode filter
            let should_decode = if let Some((start_fc, end_fc)) = self.timecode_filter {
                if let Some(timecode) = self.current_frame_timecode {
                    let passes = timecode >= start_fc && timecode < end_fc;
                    log::debug!(
                        "Timecode filter check (flush): frame {} in range [{}, {})? {}",
                        timecode, start_fc, end_fc, passes
                    );
                    passes
                } else {
                    true // No timecode available, include frame
                }
            } else {
                true // No filter set, include all frames
            };

            if !should_decode {
                log::info!(
                    "Skipping final frame outside timecode range: timecode={}",
                    self.current_frame_timecode.unwrap_or(0)
                );
                return None;
            }

            // Decode the final DST frame if we have one
            if let Some(decoder) = &mut self.dst_decoder {
                log::debug!(
                    "Flushing final DST frame: {} bytes, timecode={}",
                    self.frame_buffer.len(),
                    self.current_frame_timecode.unwrap_or(0)
                );

                // Debug: Log first bytes of frame being sent to decoder
                static DECODE_COUNT_3: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                let count3 = DECODE_COUNT_3.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if count3 < 5 {
                    let show_len = self.frame_buffer.len().min(16);
                    debug!("[DECODE_FRAME_FLUSH #{}] Passing to decoder: len={}, first_16_bytes={:02x?}",
                             count3, self.frame_buffer.len(), &self.frame_buffer[..show_len]);
                }

                let mut dsd_buffer = vec![0u8; decoder.dsd_frame_bytes()];
                match decoder.decode_frame(&self.frame_buffer, &mut dsd_buffer) {
                    Ok(bytes_decoded) => {
                        dsd_buffer.truncate(bytes_decoded);
                        self.frame_buffer.clear();
                        self.total_bytes += bytes_decoded as u64;
                        Some(dsd_buffer)
                    }
                    Err(e) => {
                        log::warn!("Failed to decode final DST frame: {}", e);
                        None
                    }
                }
            } else {
                log::warn!("No DST decoder available for flush");
                None
            }
        } else {
            // Uncompressed DSD - return as-is
            let frame_data = std::mem::take(&mut self.frame_buffer);
            self.total_bytes += frame_data.len() as u64;
            Some(frame_data)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scarletbook::types::FrameFormat;
    use std::path::PathBuf;

    const SECTOR_SIZE: usize = 2048;

    fn fixture(name: &str) -> Vec<u8> {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/dst_frames")
            .join(name);
        std::fs::read(&p).unwrap_or_else(|e| panic!("missing fixture {}: {}", p.display(), e))
    }

    fn sector_header(frame_info_count: u8, packet_info_count: u8) -> u8 {
        // dst_encoded=1 (bit 0), frame_info_count (bits 2-4), packet_info_count (bits 5-7)
        1 | ((frame_info_count & 7) << 2) | ((packet_info_count & 7) << 5)
    }

    fn pkt_info(frame_start: bool, data_type: u8, len: u16) -> [u8; 2] {
        let b0 = ((frame_start as u8) << 7) | ((data_type & 7) << 3) | ((len >> 8) as u8 & 7);
        let b1 = (len & 0xFF) as u8;
        [b0, b1]
    }

    fn frm_info(m: u8, s: u8, f: u8, sc: u8) -> [u8; 4] {
        // sector_count occupies bits 2-6 of byte 3.
        [m, s, f, (sc & 0x1F) << 2]
    }

    /// Regression test for the off-by-one in `sector_count` when the first
    /// filter check accepts a frame mid-sector and the parser early-returns.
    ///
    /// Layout (5 sectors holding 3 fixture frames A, B, C):
    ///   sector 0: 1 pkt frame_start(A, sc=2), 2041B of A
    ///   sector 1: 2 pkts: rest of A (1057B) + frame_start(B, sc=2), 982B of B
    ///   sector 2: 2 pkts: rest of B (1801B) + frame_start(C, sc=3), 238B of C
    ///   sector 3: 1 pkt continuation of C (2045B)
    ///   sector 4: 1 pkt continuation of C (677B) — completes C
    ///
    /// Without the fix, the early return after decoding A leaves B's sector_count
    /// one too high. When C's frame_start arrives in sector 2, the
    /// `sector_count == 0` precondition fails, B's buffer is clobbered, and B is
    /// silently dropped. With the fix all three frames decode.
    #[test]
    fn early_return_preserves_sector_count_for_following_frame() {
        let frame_a = fixture("frame_000.dst");
        let frame_b = fixture("frame_001.dst");
        let frame_c = fixture("frame_002.dst");
        assert_eq!(frame_a.len(), 3098);
        assert_eq!(frame_b.len(), 2783);
        assert_eq!(frame_c.len(), 2956);

        let mut sectors: Vec<Vec<u8>> = Vec::new();

        // ---- Sector 0: frame_start A, sc=2, 2041B audio.
        let mut s = Vec::with_capacity(SECTOR_SIZE);
        s.push(sector_header(1, 1));
        s.extend_from_slice(&pkt_info(true, 2, 2041));
        s.extend_from_slice(&frm_info(0, 0, 0, 2));
        s.extend_from_slice(&frame_a[..2041]);
        assert_eq!(s.len(), SECTOR_SIZE);
        sectors.push(s);

        // ---- Sector 1: cont A (1057B) + frame_start B (sc=2), 982B of B.
        let mut s = Vec::with_capacity(SECTOR_SIZE);
        s.push(sector_header(1, 2));
        s.extend_from_slice(&pkt_info(false, 2, 1057));
        s.extend_from_slice(&pkt_info(true, 2, 982));
        s.extend_from_slice(&frm_info(0, 0, 1, 2));
        s.extend_from_slice(&frame_a[2041..]);
        s.extend_from_slice(&frame_b[..982]);
        assert_eq!(s.len(), SECTOR_SIZE);
        sectors.push(s);

        // ---- Sector 2: cont B (1801B) + frame_start C (sc=3), 238B of C.
        let mut s = Vec::with_capacity(SECTOR_SIZE);
        s.push(sector_header(1, 2));
        s.extend_from_slice(&pkt_info(false, 2, 1801));
        s.extend_from_slice(&pkt_info(true, 2, 238));
        s.extend_from_slice(&frm_info(0, 0, 2, 3));
        s.extend_from_slice(&frame_b[982..]);
        s.extend_from_slice(&frame_c[..238]);
        assert_eq!(s.len(), SECTOR_SIZE);
        sectors.push(s);

        // ---- Sector 3: 1 pkt continuation of C (2045B).
        let mut s = Vec::with_capacity(SECTOR_SIZE);
        s.push(sector_header(0, 1));
        s.extend_from_slice(&pkt_info(false, 2, 2045));
        s.extend_from_slice(&frame_c[238..238 + 2045]);
        assert_eq!(s.len(), SECTOR_SIZE);
        sectors.push(s);

        // ---- Sector 4: 1 pkt completing C (677B remaining).
        let used = 238 + 2045;
        let remaining = frame_c.len() - used;
        assert_eq!(remaining, 673);
        let mut s = Vec::with_capacity(SECTOR_SIZE);
        s.push(sector_header(0, 1));
        s.extend_from_slice(&pkt_info(false, 2, remaining as u16));
        s.extend_from_slice(&frame_c[used..]);
        // Pad the rest of the sector — real SACD sectors are exactly 2048 bytes.
        s.resize(SECTOR_SIZE, 0);
        assert_eq!(s.len(), SECTOR_SIZE);
        sectors.push(s);

        // Drive the parser. Use a wide filter so all three frames are accepted.
        let mut parser =
            AudioSectorParser::new(FrameFormat::Dst, 2, 2_822_400).expect("parser create");
        parser.set_timecode_filter(0, 0, 0, 0, 0, 100); // [0, 100)

        let mut produced = 0usize;
        for sector in &sectors {
            if parser.parse_sector(sector).expect("parse_sector").is_some() {
                produced += 1;
            }
        }
        if parser.flush().is_some() {
            produced += 1;
        }

        // The bug drops frame B (every other frame), so without the fix we'd
        // see only 1 (A) — possibly 2 if C still completes via end-of-sector.
        // With the fix, all three frames must decode.
        assert_eq!(
            parser.decoded_frames, 3,
            "expected 3 decoded frames; got {} (filtered={}). \
             The sector_count off-by-one bug drops frame B in the early-return path.",
            parser.decoded_frames, parser.filtered_frames
        );
        assert_eq!(produced, 3);
    }
}
