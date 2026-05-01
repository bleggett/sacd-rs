// DST (Direct Stream Transfer) Decoder
//
// Pure Rust port of the MPEG-4 DST decoder for lossless DSD compression.
// Tracks the reference C implementation in sacd-ripper/libs/libdstdec.
//
// MPEG-4 Audio RM Module - Lossless coding of 1-bit oversampled audio
// ISO/IEC 14496-3:2001/Amd 6:2005

use crate::scarletbook::consts::{DSD64_SAMPLE_RATE, DSD128_SAMPLE_RATE, DSD256_SAMPLE_RATE};
use anyhow::{Result, bail};

// ============================================================================
// CONSTANTS — match conststr.h
// ============================================================================

const RESOL: i64 = 8;

const SIZE_CODEDPREDORDER: usize = 7;
const SIZE_PREDCOEF: usize = 9;

const AC_BITS: usize = 8;
const AC_PROBS: i32 = 1 << AC_BITS;
const AC_HISBITS: usize = 6;
const AC_HISMAX: usize = 1 << AC_HISBITS;
const AC_QSTEP: usize = SIZE_PREDCOEF - AC_HISBITS; // 3

const NROFFRICEMETHODS: usize = 3;
const NROFPRICEMETHODS: usize = 3;
const MAXCPREDORDER: usize = 3;
const SIZE_RICEMETHOD: usize = 2;
const SIZE_RICEM: usize = 3;

const MAXNROF_FSEGS: i32 = 4;
const MAXNROF_PSEGS: i32 = 8;
const MIN_FSEG_LEN: i32 = 1024;
const MIN_PSEG_LEN: i32 = 32;

pub const MAX_CHANNELS: usize = 6;
const MAXNROF_SEGS: usize = 8;

// AC encoder register sizes
const PBITS: usize = AC_BITS;
const NBITS: usize = 4;
const ABITS: usize = PBITS + NBITS; // 12
const ONE: u32 = 1 << ABITS;
const HALF: u32 = 1 << (ABITS - 1);

// MaxNrOfFilters / MaxNrOfPtables both equal 2 * NrOfChannels in C.
fn max_nr_of_filters(nr_channels: usize) -> usize {
    2 * nr_channels
}
fn max_nr_of_ptables(nr_channels: usize) -> usize {
    2 * nr_channels
}

// ============================================================================
// ERRORS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DstError {
    NegativeBitAllocation,
    TooManySegments,
    InvalidSegmentResolution,
    InvalidSegmentLength,
    TooManyTables,
    InvalidTableNumber,
    InvalidChannelMapping,
    SegmentNumberMismatch,
    InvalidCoefficientCoding,
    InvalidCoefficientRange,
    InvalidPtableCoding,
    InvalidPtableRange,
    InvalidStuffingPattern,
    InvalidArithmeticCode,
    ArithmeticDecoder,
}

impl DstError {
    fn message(self) -> &'static str {
        match self {
            DstError::NegativeBitAllocation => "A negative number of bits allocated",
            DstError::TooManySegments => "Too many segments for this channel",
            DstError::InvalidSegmentResolution => "Invalid segment resolution",
            DstError::InvalidSegmentLength => "Invalid segment length",
            DstError::TooManyTables => "Too many tables for this frame",
            DstError::InvalidTableNumber => "Invalid table number for segment",
            DstError::InvalidChannelMapping => "Mapping can't be the same for all channels",
            DstError::SegmentNumberMismatch => {
                "Not same number of segments for filters and Ptables"
            }
            DstError::InvalidCoefficientCoding => "Invalid coefficient coding method",
            DstError::InvalidCoefficientRange => "Filter coefficient out of range",
            DstError::InvalidPtableCoding => "Invalid Ptable coding method",
            DstError::InvalidPtableRange => "Ptable entry out of range",
            DstError::InvalidStuffingPattern => "Illegal stuffing pattern",
            DstError::InvalidArithmeticCode => "Illegal arithmetic code",
            DstError::ArithmeticDecoder => "Arithmetic decoding error",
        }
    }
}

impl std::fmt::Display for DstError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl std::error::Error for DstError {}

// ============================================================================
// BIT READER — matches FIO_BitGet* / getbits()
// ============================================================================

struct BitReader<'a> {
    data: &'a [u8],
    /// Bit position (0..8). 0 means a fresh byte must be loaded.
    bit_position: u8,
    /// Index of next byte to read from `data`.
    byte_counter: usize,
    data_byte: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            bit_position: 0,
            byte_counter: 0,
            data_byte: 0,
        }
    }

    fn fetch_byte(&mut self) -> Result<()> {
        if self.byte_counter >= self.data.len() {
            bail!(DstError::NegativeBitAllocation);
        }
        self.data_byte = self.data[self.byte_counter];
        self.byte_counter += 1;
        self.bit_position = 8;
        Ok(())
    }

    fn read_bit(&mut self) -> Result<u8> {
        if self.bit_position == 0 {
            self.fetch_byte()?;
        }
        self.bit_position -= 1;
        Ok((self.data_byte >> self.bit_position) & 1)
    }

    fn read_uint(&mut self, len: usize) -> Result<u32> {
        if len == 0 {
            return Ok(0);
        }
        let mut out: u32 = 0;
        let mut remaining = len;
        while remaining > 0 {
            if self.bit_position == 0 {
                self.fetch_byte()?;
            }
            let take = (self.bit_position as usize).min(remaining);
            let shift = self.bit_position as usize - take;
            let mask: u32 = ((1u32 << take) - 1) << shift;
            let bits = ((self.data_byte as u32) & mask) >> shift;
            out = (out << take) | bits;
            self.bit_position -= take as u8;
            remaining -= take;
        }
        Ok(out)
    }

    fn read_int(&mut self, len: usize) -> Result<i32> {
        let mut v = self.read_uint(len)? as i32;
        if len > 0 && v >= (1 << (len - 1)) {
            v -= 1 << len;
        }
        Ok(v)
    }

    fn read_short_signed(&mut self, len: usize) -> Result<i16> {
        Ok(self.read_int(len)? as i16)
    }

    fn read_byte(&mut self) -> Result<u8> {
        Ok(self.read_uint(8)? as u8)
    }

    fn bit_count(&self) -> i64 {
        self.byte_counter as i64 * 8 - self.bit_position as i64
    }
}

// ============================================================================
// AC DECODER — matches dst_ac.c / dst_fram.c LT_ACDecodeBit_*
// ============================================================================

#[derive(Default)]
struct AcData {
    a: u32,
    c: u32,
    cbptr: i32,
}

impl AcData {
    fn init(&mut self, cb: &[u8], fs: i32) {
        self.a = ONE - 1;
        self.c = 0;
        self.cbptr = 1;
        while self.cbptr <= ABITS as i32 {
            self.c <<= 1;
            if self.cbptr < fs {
                self.c |= cb[self.cbptr as usize] as u32;
            }
            self.cbptr += 1;
        }
    }

    #[inline]
    fn decode_bit(&mut self, p: i32, cb: &[u8], fs: i32) -> u8 {
        // approximate (A * p) with "partial rounding"
        let ap = ((self.a >> PBITS) | ((self.a >> (PBITS - 1)) & 1)) * p as u32;
        let h = self.a - ap;
        let b = if self.c >= h {
            self.c -= h;
            self.a = ap;
            0u8
        } else {
            self.a = h;
            1u8
        };
        // Renormalize. `cb` is zero-padded past `fs` by the caller, so the
        // bounds check the C reference does (`if cbptr < fs`) is unnecessary
        // — past-end reads return 0, matching the spec's "insert zero in LSB
        // of C" rule.
        let _ = fs;
        while self.a < HALF {
            self.a <<= 1;
            self.c = (self.c << 1) | cb[self.cbptr as usize] as u32;
            self.cbptr += 1;
        }
        b
    }

    /// Validate trailing bits at end-of-frame. Returns 1 on success (matches C).
    fn flush(&mut self, cb: &[u8], fs: i32) -> u8 {
        let mut b: u8;
        if self.cbptr < fs - 7 {
            b = 0;
        } else {
            b = 1;
            while self.cbptr < fs && b == 1 {
                if cb[self.cbptr as usize] != 0 {
                    b = 1;
                }
                self.cbptr += 1;
            }
        }
        b
    }
}

#[inline]
fn ac_get_ptable_index(predict: i16, ptable_len: i32) -> i32 {
    let abs = predict.unsigned_abs() as i32;
    let j = abs >> AC_QSTEP;
    if j >= ptable_len { ptable_len - 1 } else { j }
}

// ============================================================================
// REVERSE 7 LSB TABLE — matches Reverse7LSBs / reverse[]
// ============================================================================

const REVERSE7: [i16; 128] = [
    1, 65, 33, 97, 17, 81, 49, 113, 9, 73, 41, 105, 25, 89, 57, 121, 5, 69, 37, 101, 21, 85, 53,
    117, 13, 77, 45, 109, 29, 93, 61, 125, 3, 67, 35, 99, 19, 83, 51, 115, 11, 75, 43, 107, 27, 91,
    59, 123, 7, 71, 39, 103, 23, 87, 55, 119, 15, 79, 47, 111, 31, 95, 63, 127, 2, 66, 34, 98, 18,
    82, 50, 114, 10, 74, 42, 106, 26, 90, 58, 122, 6, 70, 38, 102, 22, 86, 54, 118, 14, 78, 46,
    110, 30, 94, 62, 126, 4, 68, 36, 100, 20, 84, 52, 116, 12, 76, 44, 108, 28, 92, 60, 124, 8, 72,
    40, 104, 24, 88, 56, 120, 16, 80, 48, 112, 32, 96, 64, 128,
];

fn reverse7_lsbs(c: i16) -> i32 {
    REVERSE7[((c as i32 + (1 << SIZE_PREDCOEF)) & 127) as usize] as i32
}

// ============================================================================
// DATA STRUCTURES
// ============================================================================

#[derive(Clone)]
struct Segment {
    resolution: i32,
    segment_len: [[i32; MAXNROF_SEGS]; MAX_CHANNELS],
    nr_of_segments: [i32; MAX_CHANNELS],
    table4_segment: [[i32; MAXNROF_SEGS]; MAX_CHANNELS],
}

impl Default for Segment {
    fn default() -> Self {
        Self {
            resolution: 0,
            segment_len: [[0; MAXNROF_SEGS]; MAX_CHANNELS],
            nr_of_segments: [0; MAX_CHANNELS],
            table4_segment: [[0; MAXNROF_SEGS]; MAX_CHANNELS],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TableType {
    Filter,
    Ptable,
}

struct CodedTable {
    /// CPredOrder[Method], length = max(NROFFRICEMETHODS, NROFPRICEMETHODS).
    c_pred_order: [i32; 4],
    /// CPredCoef[Method][CoefNr]
    c_pred_coef: [[i32; MAXCPREDORDER]; 4],
    /// Coded[FilterNr or PtableNr]
    coded: Vec<i32>,
    /// BestMethod[FilterNr or PtableNr]
    best_method: Vec<i32>,
    /// m[FilterNr or PtableNr][Method]
    m: Vec<[i32; 4]>,
    /// FILTER or PTABLE
    table_type: TableType,
}

impl CodedTable {
    fn new(table_type: TableType, max_tables: usize) -> Self {
        let mut t = Self {
            c_pred_order: [0; 4],
            c_pred_coef: [[0; MAXCPREDORDER]; 4],
            coded: vec![0; max_tables],
            best_method: vec![-1; max_tables],
            m: vec![[0; 4]; max_tables],
            table_type,
        };
        t.ccp_calc_init();
        t
    }

    /// CCP_CalcInit from ccp_calc.c.
    fn ccp_calc_init(&mut self) {
        match self.table_type {
            TableType::Filter => {
                // Method 0
                self.c_pred_order[0] = 1;
                self.c_pred_coef[0] = [-8, 0, 0];
                // Method 1
                self.c_pred_order[1] = 2;
                self.c_pred_coef[1] = [-16, 8, 0];
                // Method 2
                self.c_pred_order[2] = 3;
                self.c_pred_coef[2] = [-9, -5, 6];
            }
            TableType::Ptable => {
                self.c_pred_order[0] = 1;
                self.c_pred_coef[0] = [-8, 0, 0];
                self.c_pred_order[1] = 2;
                self.c_pred_coef[1] = [-16, 8, 0];
                self.c_pred_order[2] = 3;
                self.c_pred_coef[2] = [-24, 24, -8];
            }
        }
    }
}

#[derive(Default)]
struct FrameHeader {
    nr_of_channels: i32,
    nr_of_filters: i32,
    nr_of_ptables: i32,
    pred_order: Vec<i32>, // [MaxNrOfFilters]
    ptable_len: Vec<i32>, // [MaxNrOfPtables]
    /// ICoefA[FilterNr][CoefNr] padded to (1<<SIZE_CODEDPREDORDER) entries.
    i_coef_a: Vec<Vec<i16>>,
    dst_coded: i32,
    calc_nr_of_bytes: i64,
    calc_nr_of_bits: i64,
    half_prob: [i32; MAX_CHANNELS],
    nr_of_half_bits: [i32; MAX_CHANNELS],
    f_seg: Segment,
    p_seg: Segment,
    p_same_seg_as_f: i32,
    p_same_map_as_f: i32,
    f_same_seg_all_ch: i32,
    f_same_map_all_ch: i32,
    p_same_seg_all_ch: i32,
    p_same_map_all_ch: i32,
    max_nr_of_filters: i32,
    max_nr_of_ptables: i32,
    max_frame_len: i64,
    nr_of_bits_per_ch: i64,
}

// ============================================================================
// DST DECODER
// ============================================================================

pub struct DstDecoder {
    frame_hdr: FrameHeader,
    str_filter: CodedTable,
    str_ptable: CodedTable,
    /// P_one[PtableNr][EntryNr]
    p_one: Vec<[i32; AC_HISMAX]>,
    /// AData buffer: one bit per byte (0 or 1).
    a_data: Vec<u8>,
    a_data_len: i32,
    /// Filter4Bit[ChNr][BitNr], filled per-frame.
    filter4_bit: Vec<Vec<u8>>,
    /// Ptable4Bit[ChNr][BitNr], filled per-frame.
    ptable4_bit: Vec<Vec<u8>>,
    channel_count: usize,
    /// Whether the current CPU advertises AVX2; cached at construction so
    /// the per-bit FIR dispatch is one predicted branch instead of a
    /// `cpuid` call.
    has_avx2: bool,
}

impl DstDecoder {
    pub fn new(channel_count: usize, sample_rate: usize) -> Result<Self> {
        if channel_count == 0 || channel_count > MAX_CHANNELS {
            bail!("Invalid channel count: {}", channel_count);
        }
        // The C reference computes MaxFrameLen as `588 * SampleRate / 8`
        // where SampleRate is an Fs44 multiplier (64/128/256). Validate
        // and translate the Hz value here; we don't need to retain it.
        let fsample_44: i64 = match sample_rate as u32 {
            DSD64_SAMPLE_RATE => 64,
            DSD128_SAMPLE_RATE => 128,
            DSD256_SAMPLE_RATE => 256,
            _ => bail!("Unsupported sample rate: {}", sample_rate),
        };
        let max_frame_len = 588 * fsample_44 / 8;
        let nr_of_bits_per_ch = max_frame_len * RESOL;
        let max_nr_of_filters = max_nr_of_filters(channel_count);
        let max_nr_of_ptables = max_nr_of_ptables(channel_count);

        Ok(Self {
            frame_hdr: FrameHeader {
                nr_of_channels: channel_count as i32,
                pred_order: vec![0; max_nr_of_filters],
                ptable_len: vec![0; max_nr_of_ptables],
                i_coef_a: vec![vec![0; 1 << SIZE_CODEDPREDORDER]; max_nr_of_filters],
                max_nr_of_filters: max_nr_of_filters as i32,
                max_nr_of_ptables: max_nr_of_ptables as i32,
                max_frame_len,
                nr_of_bits_per_ch,
                ..Default::default()
            },
            str_filter: CodedTable::new(TableType::Filter, max_nr_of_filters),
            str_ptable: CodedTable::new(TableType::Ptable, max_nr_of_ptables),
            p_one: vec![[0; AC_HISMAX]; max_nr_of_ptables],
            a_data: Vec::new(),
            a_data_len: 0,
            filter4_bit: vec![vec![0u8; nr_of_bits_per_ch as usize]; MAX_CHANNELS],
            ptable4_bit: vec![vec![0u8; nr_of_bits_per_ch as usize]; MAX_CHANNELS],
            channel_count,
            #[cfg(target_arch = "x86_64")]
            has_avx2: std::is_x86_feature_detected!("avx2"),
            #[cfg(not(target_arch = "x86_64"))]
            has_avx2: false,
        })
    }

    /// Number of bytes the decoder writes per frame: max_frame_len * channels.
    pub fn dsd_frame_bytes(&self) -> usize {
        (self.frame_hdr.max_frame_len as usize) * self.channel_count
    }

    /// Decode one DST frame, writing channel-interleaved DSD bytes to `dsd_data`.
    /// Returns number of bytes written.
    pub fn decode_frame(&mut self, dst_data: &[u8], dsd_data: &mut [u8]) -> Result<usize> {
        let nr_of_channels = self.frame_hdr.nr_of_channels as usize;
        let max_frame_len = self.frame_hdr.max_frame_len;
        let nr_of_bits_per_ch = self.frame_hdr.nr_of_bits_per_ch as usize;
        let total_dsd_bytes = (nr_of_bits_per_ch * nr_of_channels) / 8;

        if dsd_data.len() < total_dsd_bytes {
            bail!(
                "DSD output buffer too small: need {}, got {}",
                total_dsd_bytes,
                dsd_data.len()
            );
        }

        self.frame_hdr.calc_nr_of_bytes = dst_data.len() as i64;
        self.frame_hdr.calc_nr_of_bits = self.frame_hdr.calc_nr_of_bytes * 8;

        let res = self.unpack_dst_frame(dst_data, dsd_data);
        if let Err(e) = res {
            // On error C returns DSD silence (0x55) for the whole frame.
            for b in &mut dsd_data[..total_dsd_bytes] {
                *b = 0x55;
            }
            return Err(e);
        }

        if self.frame_hdr.dst_coded == 0 {
            // Uncompressed DSD already written into dsd_data.
            return Ok((max_frame_len * nr_of_channels as i64) as usize);
        }

        // DST-coded: run filter loop.
        let bytes = self.decode_dsd_samples(dsd_data)?;
        Ok(bytes)
    }

    // ------------------------------------------------------------
    // UnpackDSTframe
    // ------------------------------------------------------------
    fn unpack_dst_frame(&mut self, dst_data: &[u8], dsd_data: &mut [u8]) -> Result<()> {
        let mut reader = BitReader::new(dst_data);

        self.frame_hdr.dst_coded = reader.read_uint(1)? as i32;

        if self.frame_hdr.dst_coded == 0 {
            // 1 bit dummy + 6 bits stuffing pattern (must be zero).
            let _ = reader.read_uint(1)?;
            let stuffing = reader.read_uint(6)?;
            if stuffing != 0 {
                bail!(DstError::InvalidStuffingPattern);
            }
            self.read_dsd_frame(&mut reader, dsd_data)?;
            return Ok(());
        }

        self.read_segment_data(&mut reader)?;
        self.read_mapping_data(&mut reader)?;
        self.read_filter_coef_sets(&mut reader)?;
        self.read_probability_tables(&mut reader)?;

        // ADataLen = total bits - bits already read.
        self.a_data_len = (self.frame_hdr.calc_nr_of_bits - reader.bit_count()) as i32;
        self.read_arithmetic_coded_data(&mut reader)?;

        if self.a_data_len > 0 && self.a_data[0] != 0 {
            bail!(DstError::InvalidArithmeticCode);
        }
        Ok(())
    }

    fn read_dsd_frame(&self, reader: &mut BitReader, dsd_data: &mut [u8]) -> Result<()> {
        let max = (self.frame_hdr.max_frame_len * self.frame_hdr.nr_of_channels as i64) as usize;
        for byte in dsd_data.iter_mut().take(max) {
            *byte = reader.read_byte()?;
        }
        Ok(())
    }

    // ------------------------------------------------------------
    // Segmentation
    // ------------------------------------------------------------
    fn read_segment_data(&mut self, reader: &mut BitReader) -> Result<()> {
        self.frame_hdr.p_same_seg_as_f = reader.read_uint(1)? as i32;

        Self::read_table_segment_data(
            reader,
            self.frame_hdr.nr_of_channels as usize,
            self.frame_hdr.max_frame_len as i32,
            MAXNROF_FSEGS,
            MIN_FSEG_LEN,
            &mut self.frame_hdr.f_seg,
            &mut self.frame_hdr.f_same_seg_all_ch,
        )?;

        if self.frame_hdr.p_same_seg_as_f == 1 {
            self.copy_segment_data()?;
        } else {
            Self::read_table_segment_data(
                reader,
                self.frame_hdr.nr_of_channels as usize,
                self.frame_hdr.max_frame_len as i32,
                MAXNROF_PSEGS,
                MIN_PSEG_LEN,
                &mut self.frame_hdr.p_seg,
                &mut self.frame_hdr.p_same_seg_all_ch,
            )?;
        }
        Ok(())
    }

    fn read_table_segment_data(
        reader: &mut BitReader,
        nr_of_channels: usize,
        frame_len: i32,
        max_nr_of_segs: i32,
        min_seg_len: i32,
        s: &mut Segment,
        same_seg_all_ch: &mut i32,
    ) -> Result<()> {
        let mut defined_bits: i32 = 0;
        let mut resol_read = false;
        let mut seg_nr: i32 = 0;
        let mut max_seg_size: i32 = frame_len - min_seg_len / 8;

        *same_seg_all_ch = reader.read_uint(1)? as i32;
        if *same_seg_all_ch == 1 {
            let mut end_of_channel = reader.read_uint(1)? as i32;
            while end_of_channel == 0 {
                if seg_nr >= max_nr_of_segs {
                    bail!(DstError::TooManySegments);
                }
                if !resol_read {
                    let nbits = log2_round_up((frame_len - min_seg_len / 8) as u32);
                    s.resolution = reader.read_uint(nbits)? as i32;
                    if s.resolution == 0 || s.resolution > frame_len - min_seg_len / 8 {
                        bail!(DstError::InvalidSegmentResolution);
                    }
                    resol_read = true;
                }
                let nbits = log2_round_up((max_seg_size / s.resolution) as u32);
                let len = reader.read_uint(nbits)? as i32;
                s.segment_len[0][seg_nr as usize] = len;
                if s.resolution * 8 * len < min_seg_len
                    || s.resolution * 8 * len > frame_len * 8 - defined_bits - min_seg_len
                {
                    bail!(DstError::InvalidSegmentLength);
                }
                defined_bits += s.resolution * 8 * len;
                max_seg_size -= s.resolution * len;
                seg_nr += 1;
                end_of_channel = reader.read_uint(1)? as i32;
            }
            s.nr_of_segments[0] = seg_nr + 1;
            s.segment_len[0][seg_nr as usize] = 0;
            for ch in 1..nr_of_channels {
                s.nr_of_segments[ch] = s.nr_of_segments[0];
                for i in 0..s.nr_of_segments[0] as usize {
                    s.segment_len[ch][i] = s.segment_len[0][i];
                }
            }
        } else {
            let mut ch_nr = 0usize;
            while ch_nr < nr_of_channels {
                if seg_nr >= max_nr_of_segs {
                    bail!(DstError::TooManySegments);
                }
                let end_of_channel = reader.read_uint(1)? as i32;
                if end_of_channel == 0 {
                    if !resol_read {
                        let nbits = log2_round_up((frame_len - min_seg_len / 8) as u32);
                        s.resolution = reader.read_uint(nbits)? as i32;
                        if s.resolution == 0 || s.resolution > frame_len - min_seg_len / 8 {
                            bail!(DstError::InvalidSegmentResolution);
                        }
                        resol_read = true;
                    }
                    let nbits = log2_round_up((max_seg_size / s.resolution) as u32);
                    let len = reader.read_uint(nbits)? as i32;
                    s.segment_len[ch_nr][seg_nr as usize] = len;
                    if s.resolution * 8 * len < min_seg_len
                        || s.resolution * 8 * len > frame_len * 8 - defined_bits - min_seg_len
                    {
                        bail!(DstError::InvalidSegmentLength);
                    }
                    defined_bits += s.resolution * 8 * len;
                    max_seg_size -= s.resolution * len;
                    seg_nr += 1;
                } else {
                    s.nr_of_segments[ch_nr] = seg_nr + 1;
                    s.segment_len[ch_nr][seg_nr as usize] = 0;
                    seg_nr = 0;
                    defined_bits = 0;
                    max_seg_size = frame_len - min_seg_len / 8;
                    ch_nr += 1;
                }
            }
        }
        if !resol_read {
            s.resolution = 1;
        }
        Ok(())
    }

    fn copy_segment_data(&mut self) -> Result<()> {
        let nr_of_channels = self.frame_hdr.nr_of_channels as usize;
        self.frame_hdr.p_seg.resolution = self.frame_hdr.f_seg.resolution;
        self.frame_hdr.p_same_seg_all_ch = 1;
        for ch in 0..nr_of_channels {
            self.frame_hdr.p_seg.nr_of_segments[ch] = self.frame_hdr.f_seg.nr_of_segments[ch];
            if self.frame_hdr.p_seg.nr_of_segments[ch] > MAXNROF_PSEGS {
                bail!(DstError::TooManySegments);
            }
            if self.frame_hdr.p_seg.nr_of_segments[ch] != self.frame_hdr.p_seg.nr_of_segments[0] {
                self.frame_hdr.p_same_seg_all_ch = 0;
            }
            for seg in 0..self.frame_hdr.p_seg.nr_of_segments[ch] as usize {
                let len = self.frame_hdr.f_seg.segment_len[ch][seg];
                self.frame_hdr.p_seg.segment_len[ch][seg] = len;
                if len != 0 && self.frame_hdr.p_seg.resolution * 8 * len < MIN_PSEG_LEN {
                    bail!(DstError::InvalidSegmentLength);
                }
                if len != self.frame_hdr.p_seg.segment_len[0][seg] {
                    self.frame_hdr.p_same_seg_all_ch = 0;
                }
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------
    // Mapping
    // ------------------------------------------------------------
    fn read_mapping_data(&mut self, reader: &mut BitReader) -> Result<()> {
        self.frame_hdr.p_same_map_as_f = reader.read_uint(1)? as i32;

        let nr_of_channels = self.frame_hdr.nr_of_channels as usize;
        let max_nr_of_filters = self.frame_hdr.max_nr_of_filters;
        let max_nr_of_ptables = self.frame_hdr.max_nr_of_ptables;

        let mut nr_of_filters = 0i32;
        Self::read_table_mapping_data(
            reader,
            nr_of_channels,
            max_nr_of_filters,
            &mut self.frame_hdr.f_seg,
            &mut nr_of_filters,
            &mut self.frame_hdr.f_same_map_all_ch,
        )?;
        self.frame_hdr.nr_of_filters = nr_of_filters;

        if self.frame_hdr.p_same_map_as_f == 1 {
            self.copy_mapping_data()?;
        } else {
            let mut nr_of_ptables = 0i32;
            Self::read_table_mapping_data(
                reader,
                nr_of_channels,
                max_nr_of_ptables,
                &mut self.frame_hdr.p_seg,
                &mut nr_of_ptables,
                &mut self.frame_hdr.p_same_map_all_ch,
            )?;
            self.frame_hdr.nr_of_ptables = nr_of_ptables;
        }

        // HalfProb per channel.
        for ch in 0..nr_of_channels {
            self.frame_hdr.half_prob[ch] = reader.read_uint(1)? as i32;
        }
        Ok(())
    }

    fn read_table_mapping_data(
        reader: &mut BitReader,
        nr_of_channels: usize,
        max_nr_of_tables: i32,
        s: &mut Segment,
        nr_of_tables: &mut i32,
        same_map_all_ch: &mut i32,
    ) -> Result<()> {
        let mut count_tables: i32 = 1;

        s.table4_segment[0][0] = 0;
        *same_map_all_ch = reader.read_uint(1)? as i32;
        if *same_map_all_ch == 1 {
            for seg_nr in 1..s.nr_of_segments[0] as usize {
                let nbits = log2_round_up(count_tables as u32);
                let v = reader.read_uint(nbits)? as i32;
                s.table4_segment[0][seg_nr] = v;
                if v == count_tables {
                    count_tables += 1;
                } else if v > count_tables {
                    bail!(DstError::InvalidTableNumber);
                }
            }
            for ch in 1..nr_of_channels {
                if s.nr_of_segments[ch] != s.nr_of_segments[0] {
                    bail!(DstError::InvalidChannelMapping);
                }
                for seg in 0..s.nr_of_segments[0] as usize {
                    s.table4_segment[ch][seg] = s.table4_segment[0][seg];
                }
            }
        } else {
            for ch_nr in 0..nr_of_channels {
                for seg_nr in 0..s.nr_of_segments[ch_nr] as usize {
                    if !(ch_nr == 0 && seg_nr == 0) {
                        let nbits = log2_round_up(count_tables as u32);
                        let v = reader.read_uint(nbits)? as i32;
                        s.table4_segment[ch_nr][seg_nr] = v;
                        if v == count_tables {
                            count_tables += 1;
                        } else if v > count_tables {
                            bail!(DstError::InvalidTableNumber);
                        }
                    }
                }
            }
        }
        if count_tables > max_nr_of_tables {
            bail!(DstError::TooManyTables);
        }
        *nr_of_tables = count_tables;
        Ok(())
    }

    fn copy_mapping_data(&mut self) -> Result<()> {
        let nr_of_channels = self.frame_hdr.nr_of_channels as usize;
        self.frame_hdr.p_same_map_all_ch = 1;
        for ch in 0..nr_of_channels {
            if self.frame_hdr.p_seg.nr_of_segments[ch] != self.frame_hdr.f_seg.nr_of_segments[ch] {
                bail!(DstError::SegmentNumberMismatch);
            }
            for seg in 0..self.frame_hdr.f_seg.nr_of_segments[ch] as usize {
                let v = self.frame_hdr.f_seg.table4_segment[ch][seg];
                self.frame_hdr.p_seg.table4_segment[ch][seg] = v;
                if v != self.frame_hdr.p_seg.table4_segment[0][seg] {
                    self.frame_hdr.p_same_map_all_ch = 0;
                }
            }
        }
        self.frame_hdr.nr_of_ptables = self.frame_hdr.nr_of_filters;
        if self.frame_hdr.nr_of_ptables > self.frame_hdr.max_nr_of_ptables {
            bail!(DstError::TooManyTables);
        }
        Ok(())
    }

    // ------------------------------------------------------------
    // Filter coefficient sets
    // ------------------------------------------------------------
    fn read_filter_coef_sets(&mut self, reader: &mut BitReader) -> Result<()> {
        let nr_of_channels = self.frame_hdr.nr_of_channels as usize;
        let nr_of_filters = self.frame_hdr.nr_of_filters as usize;
        let coef_max = 1 << SIZE_CODEDPREDORDER;

        for filter_nr in 0..nr_of_filters {
            let pred_order = reader.read_uint(SIZE_CODEDPREDORDER)? as i32 + 1;
            self.frame_hdr.pred_order[filter_nr] = pred_order;
            let coded = reader.read_uint(1)? as i32;
            self.str_filter.coded[filter_nr] = coded;

            let coefs = &mut self.frame_hdr.i_coef_a[filter_nr];
            // Zero out tail (for SSE2 path safety).
            for c in coefs.iter_mut() {
                *c = 0;
            }

            if coded == 0 {
                self.str_filter.best_method[filter_nr] = -1;
                for c in coefs.iter_mut().take(pred_order as usize) {
                    *c = reader.read_short_signed(SIZE_PREDCOEF)?;
                }
            } else {
                let best_method = reader.read_uint(SIZE_RICEMETHOD)? as i32;
                if best_method as usize >= NROFFRICEMETHODS {
                    bail!(DstError::InvalidCoefficientCoding);
                }
                self.str_filter.best_method[filter_nr] = best_method;
                let cpred_order = self.str_filter.c_pred_order[best_method as usize];
                if cpred_order >= pred_order {
                    bail!(DstError::InvalidCoefficientCoding);
                }
                for c in coefs.iter_mut().take(cpred_order as usize) {
                    *c = reader.read_short_signed(SIZE_PREDCOEF)?;
                }
                let m = reader.read_uint(SIZE_RICEM)? as i32;
                self.str_filter.m[filter_nr][best_method as usize] = m;
                for c in cpred_order as usize..pred_order as usize {
                    let mut x = 0i32;
                    for tap in 0..cpred_order as usize {
                        x += self.str_filter.c_pred_coef[best_method as usize][tap]
                            * coefs[c - tap - 1] as i32;
                    }
                    let r = rice_decode(reader, m)?;
                    let cv = if x >= 0 {
                        r - (x + 4) / 8
                    } else {
                        r + (-x + 3) / 8
                    };
                    if !(-(1 << (SIZE_PREDCOEF - 1))..(1 << (SIZE_PREDCOEF - 1))).contains(&cv) {
                        bail!(DstError::InvalidCoefficientRange);
                    }
                    coefs[c] = cv as i16;
                }
            }
            // Coefs after pred_order remain zero (already cleared).
            let _ = coef_max;
        }

        // Set NrOfHalfBits[ChNr] = PredOrder[FSeg.Table4Segment[ChNr][0]].
        for ch in 0..nr_of_channels {
            let filter_idx = self.frame_hdr.f_seg.table4_segment[ch][0] as usize;
            self.frame_hdr.nr_of_half_bits[ch] = self.frame_hdr.pred_order[filter_idx];
        }
        Ok(())
    }

    // ------------------------------------------------------------
    // Probability tables
    // ------------------------------------------------------------
    fn read_probability_tables(&mut self, reader: &mut BitReader) -> Result<()> {
        let nr_of_ptables = self.frame_hdr.nr_of_ptables as usize;

        for ptable_nr in 0..nr_of_ptables {
            let len = reader.read_uint(AC_HISBITS)? as i32 + 1;
            self.frame_hdr.ptable_len[ptable_nr] = len;
            if len > 1 {
                let coded = reader.read_uint(1)? as i32;
                self.str_ptable.coded[ptable_nr] = coded;
                if coded == 0 {
                    self.str_ptable.best_method[ptable_nr] = -1;
                    for entry in 0..len as usize {
                        let v = reader.read_uint(AC_BITS - 1)? as i32 + 1;
                        self.p_one[ptable_nr][entry] = v;
                    }
                } else {
                    let best_method = reader.read_uint(SIZE_RICEMETHOD)? as i32;
                    if best_method as usize >= NROFPRICEMETHODS {
                        bail!(DstError::InvalidPtableCoding);
                    }
                    self.str_ptable.best_method[ptable_nr] = best_method;
                    let cpred_order = self.str_ptable.c_pred_order[best_method as usize];
                    if cpred_order >= len {
                        bail!(DstError::InvalidPtableCoding);
                    }
                    for entry in 0..cpred_order as usize {
                        let v = reader.read_uint(AC_BITS - 1)? as i32 + 1;
                        self.p_one[ptable_nr][entry] = v;
                    }
                    let m = reader.read_uint(SIZE_RICEM)? as i32;
                    self.str_ptable.m[ptable_nr][best_method as usize] = m;
                    for entry in cpred_order as usize..len as usize {
                        // entry < 0 || entry > AC_HISMAX check is structurally impossible here;
                        // but the C code performs it inside the loop. We mirror its semantics:
                        // entry is non-negative and bounded by len <= AC_HISMAX, so always pass.
                        let mut x = 0i32;
                        for tap in 0..cpred_order as usize {
                            x += self.str_ptable.c_pred_coef[best_method as usize][tap]
                                * self.p_one[ptable_nr][entry - tap - 1];
                        }
                        let r = rice_decode(reader, m)?;
                        let cv = if x >= 0 {
                            r - (x + 4) / 8
                        } else {
                            r + (-x + 3) / 8
                        };
                        if !(1..=(1 << (AC_BITS - 1))).contains(&cv) {
                            bail!(DstError::InvalidPtableRange);
                        }
                        self.p_one[ptable_nr][entry] = cv;
                    }
                }
            } else {
                self.p_one[ptable_nr][0] = 128;
                self.str_ptable.best_method[ptable_nr] = -1;
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------
    // Arithmetic-coded data: read bit-by-bit into a_data[].
    // ------------------------------------------------------------
    fn read_arithmetic_coded_data(&mut self, reader: &mut BitReader) -> Result<()> {
        let n = self.a_data_len.max(0) as usize;
        self.a_data.clear();
        // Reserve room for the AC bits + a small zero-padded tail so
        // `decode_bit` can read past the end without a bounds check. The
        // renormalise loop reads at most ABITS bits beyond cbptr, plus the
        // flush walk; 64 zero bytes are plenty.
        self.a_data.resize(n + 64, 0);
        // Fast path: pull 8 bits at a time and expand them MSB-first to
        // a-data bytes. `read_byte` (= `read_uint(8)`) handles arbitrary
        // bit alignment — it spans byte boundaries when needed — so this
        // produces the exact same per-bit sequence as the slow path. The
        // trailing `while i < n` covers when `a_data_len` is not a
        // multiple of 8.
        let mut i = 0;
        while i + 8 <= n {
            let byte = reader.read_byte()? as u32;
            self.a_data[i] = ((byte >> 7) & 1) as u8;
            self.a_data[i + 1] = ((byte >> 6) & 1) as u8;
            self.a_data[i + 2] = ((byte >> 5) & 1) as u8;
            self.a_data[i + 3] = ((byte >> 4) & 1) as u8;
            self.a_data[i + 4] = ((byte >> 3) & 1) as u8;
            self.a_data[i + 5] = ((byte >> 2) & 1) as u8;
            self.a_data[i + 6] = ((byte >> 1) & 1) as u8;
            self.a_data[i + 7] = (byte & 1) as u8;
            i += 8;
        }
        while i < n {
            self.a_data[i] = reader.read_bit()?;
            i += 1;
        }
        Ok(())
    }

    // ------------------------------------------------------------
    // FillTable4Bit
    // ------------------------------------------------------------
    fn fill_table_4bit(
        nr_of_channels: usize,
        nr_of_bits_per_ch: usize,
        s: &Segment,
        table_4bit: &mut [Vec<u8>],
    ) {
        for (ch, dst) in table_4bit.iter_mut().take(nr_of_channels).enumerate() {
            let mut start = 0usize;
            let mut last_seg = 0usize;
            let n = s.nr_of_segments[ch] as usize;
            for seg in 0..n.saturating_sub(1) {
                let val = s.table4_segment[ch][seg] as u8;
                let end = start + (s.resolution as usize) * 8 * (s.segment_len[ch][seg] as usize);
                dst[start..end].fill(val);
                start = end;
                last_seg = seg + 1;
            }
            // Final segment fills the rest.
            let val = s.table4_segment[ch][last_seg] as u8;
            dst[start..nr_of_bits_per_ch].fill(val);
        }
    }

    // ------------------------------------------------------------
    // FIR filter lookup tables — LT_InitCoefTablesI
    // i_coef_i[FilterNr][TableNr (0..16)][StatusByte (0..256)]
    // ------------------------------------------------------------
    fn build_filter_tables(&self) -> Vec<[[i16; 256]; 16]> {
        let nr_of_filters = self.frame_hdr.nr_of_filters as usize;
        let mut out = vec![[[0i16; 256]; 16]; nr_of_filters];
        for (filter_nr, filter_tables) in out.iter_mut().enumerate() {
            let filter_length = self.frame_hdr.pred_order[filter_nr];
            let coefs = &self.frame_hdr.i_coef_a[filter_nr];
            for (table_nr, table) in filter_tables.iter_mut().enumerate() {
                let k = (filter_length - (table_nr as i32) * 8).clamp(0, 8) as usize;
                for (i, slot) in table.iter_mut().enumerate() {
                    let mut cvalue: i32 = 0;
                    for j in 0..k {
                        let bit_val = (((i >> j) & 1) as i32) * 2 - 1;
                        cvalue += bit_val * coefs[table_nr * 8 + j] as i32;
                    }
                    *slot = cvalue as i16;
                }
            }
        }
        out
    }

    // ------------------------------------------------------------
    // DST_FramDSTDecode main loop
    // ------------------------------------------------------------
    fn decode_dsd_samples(&mut self, dsd_data: &mut [u8]) -> Result<usize> {
        let nr_of_channels = self.frame_hdr.nr_of_channels as usize;
        let nr_of_bits_per_ch = self.frame_hdr.nr_of_bits_per_ch as usize;

        // FillTable4Bit for filters and ptables.
        Self::fill_table_4bit(
            nr_of_channels,
            nr_of_bits_per_ch,
            &self.frame_hdr.f_seg,
            &mut self.filter4_bit,
        );
        Self::fill_table_4bit(
            nr_of_channels,
            nr_of_bits_per_ch,
            &self.frame_hdr.p_seg,
            &mut self.ptable4_bit,
        );

        // Build FIR lookup tables.
        let i_coef_i = self.build_filter_tables();

        // Status as 4 u32 words per channel (matches the C macro that shifts
        // 32-bit words). Initial value: all bytes 0xAA → each word = 0xAAAAAAAA.
        let mut status: Vec<[u32; 4]> = vec![[0xAAAA_AAAAu32; 4]; nr_of_channels];

        // Init AC and decode the first dummy bit using Reverse7LSBs(ICoefA[0][0]).
        let mut ac = AcData::default();
        ac.init(&self.a_data, self.a_data_len);
        let dummy_prob = reverse7_lsbs(self.frame_hdr.i_coef_a[0][0]);
        let _dummy = ac.decode_bit(dummy_prob, &self.a_data, self.a_data_len);

        let total_bytes = (nr_of_bits_per_ch * nr_of_channels) / 8;
        for b in &mut dsd_data[..total_bytes] {
            *b = 0;
        }

        // Hoist field accesses out of the inner loop so the borrow checker
        // and codegen don't have to chase `self.frame_hdr.*` on every bit.
        let half_prob = self.frame_hdr.half_prob;
        let nr_of_half_bits = self.frame_hdr.nr_of_half_bits;
        let ptable_len = &self.frame_hdr.ptable_len[..];
        let p_one = &self.p_one[..];
        let a_data: &[u8] = &self.a_data;
        let a_data_len: i32 = self.a_data_len;
        let filter4_bit = &self.filter4_bit[..];
        let ptable4_bit = &self.ptable4_bit[..];
        let has_avx2 = self.has_avx2;

        for bit_nr in 0..nr_of_bits_per_ch {
            let byte_nr = bit_nr / 8;
            let bit_shift = 7 - (bit_nr % 8);
            for ch_nr in 0..nr_of_channels {
                let filter_idx = filter4_bit[ch_nr][bit_nr] as usize;
                let ftable = &i_coef_i[filter_idx];
                let st = &mut status[ch_nr];

                // FIR filter (LT_RUN_FILTER_I). Each `st[i]` u32 packs four
                // status bytes (little-endian: byte j lives at
                // `(j & 3) * 8`). The implementations gather 16 i16 lookups
                // (one per status byte × table 0..15) and accumulate with
                // wrapping i16 add — bit-identical to the chained scalar
                // form across all paths.
                let predict: i16 = {
                    #[cfg(target_arch = "x86_64")]
                    {
                        if has_avx2 {
                            // SAFETY: `has_avx2` was set from
                            // `is_x86_feature_detected!("avx2")` at
                            // construction; AVX2 is available on this CPU.
                            unsafe { fir_predict_avx2(ftable, st) }
                        } else {
                            // SSE2 is baseline on every x86_64 target.
                            fir_predict_sse2(ftable, st)
                        }
                    }
                    #[cfg(not(target_arch = "x86_64"))]
                    {
                        fir_predict_scalar(ftable, st)
                    }
                };

                // Decode residual.
                let residual = if half_prob[ch_nr] != 0 && (bit_nr as i32) < nr_of_half_bits[ch_nr]
                {
                    ac.decode_bit(AC_PROBS / 2, a_data, a_data_len)
                } else {
                    let ptable_idx = ptable4_bit[ch_nr][bit_nr] as usize;
                    let plen = ptable_len[ptable_idx];
                    let entry = ac_get_ptable_index(predict, plen) as usize;
                    let prob = p_one[ptable_idx][entry];
                    ac.decode_bit(prob, a_data, a_data_len)
                };

                let bit_val = (((predict as u16) >> 15) ^ residual as u16) & 1;
                dsd_data[byte_nr * nr_of_channels + ch_nr] |= (bit_val as u8) << bit_shift;

                // Filter status update — 32-bit shift-with-carry, four words.
                let w0 = st[0];
                let w1 = st[1];
                let w2 = st[2];
                st[3] = (st[3] << 1) | (w2 >> 31);
                st[2] = (w2 << 1) | (w1 >> 31);
                st[1] = (w1 << 1) | (w0 >> 31);
                st[0] = (w0 << 1) | bit_val as u32;
            }
        }

        let ac_status = ac.flush(a_data, a_data_len);
        if ac_status != 1 {
            bail!(DstError::ArithmeticDecoder);
        }
        Ok(total_bytes)
    }
}

// ============================================================================
// FIR predict — three implementations selected at runtime.
//
// Each consumes the channel's 16-byte filter status (packed into a `[u32; 4]`
// little-endian) and returns the wrapping i16 sum
// `Σ ftable[t][status_byte_t]` for t in 0..16. The status byte at position
// `t` lives in `st[t >> 2]` at bit offset `(t & 3) * 8`.
//
// SSE2 and AVX2 paths use `_mm_add_epi16` / `_mm256_add_epi16`, which are
// per-lane modular i16 addition — bit-identical to chained `i16::wrapping_add`.
// ============================================================================

#[cfg(not(target_arch = "x86_64"))]
#[inline]
fn fir_predict_scalar(ftable: &[[i16; 256]; 16], st: &[u32; 4]) -> i16 {
    let w0 = st[0];
    let w1 = st[1];
    let w2 = st[2];
    let w3 = st[3];
    ftable[0][(w0 & 0xff) as usize]
        .wrapping_add(ftable[1][((w0 >> 8) & 0xff) as usize])
        .wrapping_add(ftable[2][((w0 >> 16) & 0xff) as usize])
        .wrapping_add(ftable[3][((w0 >> 24) & 0xff) as usize])
        .wrapping_add(ftable[4][(w1 & 0xff) as usize])
        .wrapping_add(ftable[5][((w1 >> 8) & 0xff) as usize])
        .wrapping_add(ftable[6][((w1 >> 16) & 0xff) as usize])
        .wrapping_add(ftable[7][((w1 >> 24) & 0xff) as usize])
        .wrapping_add(ftable[8][(w2 & 0xff) as usize])
        .wrapping_add(ftable[9][((w2 >> 8) & 0xff) as usize])
        .wrapping_add(ftable[10][((w2 >> 16) & 0xff) as usize])
        .wrapping_add(ftable[11][((w2 >> 24) & 0xff) as usize])
        .wrapping_add(ftable[12][(w3 & 0xff) as usize])
        .wrapping_add(ftable[13][((w3 >> 8) & 0xff) as usize])
        .wrapping_add(ftable[14][((w3 >> 16) & 0xff) as usize])
        .wrapping_add(ftable[15][((w3 >> 24) & 0xff) as usize])
}

#[cfg(target_arch = "x86_64")]
#[inline]
fn fir_predict_sse2(ftable: &[[i16; 256]; 16], st: &[u32; 4]) -> i16 {
    use std::arch::x86_64::*;
    // SAFETY: every x86_64 baseline target has SSE2.
    unsafe {
        let w0 = st[0];
        let w1 = st[1];
        let w2 = st[2];
        let w3 = st[3];
        // Two 8×i16 vectors of lookups.
        let v0 = _mm_set_epi16(
            ftable[7][((w1 >> 24) & 0xff) as usize],
            ftable[6][((w1 >> 16) & 0xff) as usize],
            ftable[5][((w1 >> 8) & 0xff) as usize],
            ftable[4][(w1 & 0xff) as usize],
            ftable[3][((w0 >> 24) & 0xff) as usize],
            ftable[2][((w0 >> 16) & 0xff) as usize],
            ftable[1][((w0 >> 8) & 0xff) as usize],
            ftable[0][(w0 & 0xff) as usize],
        );
        let v1 = _mm_set_epi16(
            ftable[15][((w3 >> 24) & 0xff) as usize],
            ftable[14][((w3 >> 16) & 0xff) as usize],
            ftable[13][((w3 >> 8) & 0xff) as usize],
            ftable[12][(w3 & 0xff) as usize],
            ftable[11][((w2 >> 24) & 0xff) as usize],
            ftable[10][((w2 >> 16) & 0xff) as usize],
            ftable[9][((w2 >> 8) & 0xff) as usize],
            ftable[8][(w2 & 0xff) as usize],
        );
        // Pairwise → 8 lanes, then horizontal-reduce 8 → 1.
        let s = _mm_add_epi16(v0, v1);
        let s = _mm_add_epi16(s, _mm_srli_si128(s, 8));
        let s = _mm_add_epi16(s, _mm_srli_si128(s, 4));
        let s = _mm_add_epi16(s, _mm_srli_si128(s, 2));
        _mm_extract_epi16::<0>(s) as i16
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
#[inline]
unsafe fn fir_predict_avx2(ftable: &[[i16; 256]; 16], st: &[u32; 4]) -> i16 {
    use std::arch::x86_64::*;
    let w0 = st[0];
    let w1 = st[1];
    let w2 = st[2];
    let w3 = st[3];
    // All 16 lookups packed into one 256-bit vector.
    let v = _mm256_set_epi16(
        ftable[15][((w3 >> 24) & 0xff) as usize],
        ftable[14][((w3 >> 16) & 0xff) as usize],
        ftable[13][((w3 >> 8) & 0xff) as usize],
        ftable[12][(w3 & 0xff) as usize],
        ftable[11][((w2 >> 24) & 0xff) as usize],
        ftable[10][((w2 >> 16) & 0xff) as usize],
        ftable[9][((w2 >> 8) & 0xff) as usize],
        ftable[8][(w2 & 0xff) as usize],
        ftable[7][((w1 >> 24) & 0xff) as usize],
        ftable[6][((w1 >> 16) & 0xff) as usize],
        ftable[5][((w1 >> 8) & 0xff) as usize],
        ftable[4][(w1 & 0xff) as usize],
        ftable[3][((w0 >> 24) & 0xff) as usize],
        ftable[2][((w0 >> 16) & 0xff) as usize],
        ftable[1][((w0 >> 8) & 0xff) as usize],
        ftable[0][(w0 & 0xff) as usize],
    );
    // Fold 16 → 8 i16 lanes (pairwise add of high and low halves), then
    // horizontal-reduce 8 → 1 with SSE2 shuffles.
    let lo = _mm256_castsi256_si128(v);
    let hi = _mm256_extracti128_si256::<1>(v);
    let s = _mm_add_epi16(lo, hi);
    let s = _mm_add_epi16(s, _mm_srli_si128(s, 8));
    let s = _mm_add_epi16(s, _mm_srli_si128(s, 4));
    let s = _mm_add_epi16(s, _mm_srli_si128(s, 2));
    _mm_extract_epi16::<0>(s) as i16
}

// ============================================================================
// Helpers
// ============================================================================

fn log2_round_up(x: u32) -> usize {
    // Smallest y with 2^y > x. Mirrors C Log2RoundUp.
    let mut y = 0usize;
    while x >= (1u32 << y) {
        y += 1;
    }
    y
}

fn rice_decode(reader: &mut BitReader, m: i32) -> Result<i32> {
    // Run length: count leading zeros until a 1 bit.
    let mut run_length = 0i32;
    loop {
        let b = reader.read_uint(1)? as i32;
        run_length += 1 - b;
        if b == 1 {
            break;
        }
    }
    let lsbs = reader.read_uint(m as usize)? as i32;
    let mut nr = (run_length << m) + lsbs;
    if nr != 0 {
        let sign = reader.read_uint(1)? as i32;
        if sign == 1 {
            nr = -nr;
        }
    }
    Ok(nr)
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decoder_creation() {
        let d = DstDecoder::new(2, 2_822_400).unwrap();
        assert_eq!(d.channel_count, 2);
        assert_eq!(d.frame_hdr.max_frame_len, 4704);
    }

    #[test]
    fn test_invalid_args() {
        assert!(DstDecoder::new(0, 2_822_400).is_err());
        assert!(DstDecoder::new(MAX_CHANNELS + 1, 2_822_400).is_err());
        assert!(DstDecoder::new(2, 48000).is_err());
    }

    #[test]
    fn test_log2_round_up() {
        // Matches C: smallest y with x < 2^y, but counted as `while x >= 1<<y`.
        assert_eq!(log2_round_up(0), 0);
        assert_eq!(log2_round_up(1), 1);
        assert_eq!(log2_round_up(2), 2);
        assert_eq!(log2_round_up(3), 2);
        assert_eq!(log2_round_up(4), 3);
        assert_eq!(log2_round_up(7), 3);
        assert_eq!(log2_round_up(8), 4);
    }

    #[test]
    fn test_ac_get_ptable_index() {
        assert_eq!(ac_get_ptable_index(0, 10), 0);
        assert_eq!(ac_get_ptable_index(8, 10), 1);
        assert_eq!(ac_get_ptable_index(-8, 10), 1);
        assert_eq!(ac_get_ptable_index(1000, 5), 4); // clamped
    }

    #[test]
    fn test_reverse7_lsbs() {
        assert_eq!(reverse7_lsbs(0), 1);
    }

    #[test]
    fn test_bit_reader_basic() {
        let data = [0b1010_0110u8, 0b1111_0000];
        let mut r = BitReader::new(&data);
        assert_eq!(r.read_uint(1).unwrap(), 1);
        assert_eq!(r.read_uint(1).unwrap(), 0);
        assert_eq!(r.read_uint(2).unwrap(), 0b10);
        assert_eq!(r.read_uint(4).unwrap(), 0b0110);
        assert_eq!(r.read_uint(4).unwrap(), 0b1111);
        assert_eq!(r.read_uint(4).unwrap(), 0);
        assert!(r.read_uint(1).is_err());
    }

    #[test]
    fn test_bit_reader_signed() {
        // 9 bits of 0b1_1110_0101 = 485 unsigned = -27 signed
        let data = [0b1111_0010u8, 0b1000_0000];
        let mut r = BitReader::new(&data);
        assert_eq!(r.read_int(9).unwrap(), -27);
    }
}
