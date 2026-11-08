//! RIFF/WAVE PCM audio decoder strictly matching canonical GoldSrc / Xash3d sound engine specs.
//!
//! Supports 8-bit unsigned and 16-bit signed PCM audio at 11025, 22050, and 44100 Hz,
//! mono and stereo, with zero-copy header validation and cue/loop parsing.

use crate::error::AudioError;
use zerocopy::{FromBytes, Immutable, KnownLayout, LittleEndian, U16, U32};

/// 8-byte standard RIFF chunk header layout.
#[derive(Debug, Clone, Copy, FromBytes, KnownLayout, Immutable)]
#[repr(C)]
pub struct RiffChunkHeader {
    /// 4-byte FourCC identifier (e.g., `RIFF`, `fmt `, `data`, `cue `).
    pub id: [u8; 4],
    /// 32-bit little-endian size of the chunk payload (excluding header).
    pub size: U32<LittleEndian>,
}

/// Standard Windows PCM `WAVEFORMATEX` header structure (at least 16 bytes).
#[derive(Debug, Clone, Copy, FromBytes, KnownLayout, Immutable)]
#[repr(C)]
pub struct WaveFmtPcm {
    /// Format code (1 = WAVE_FORMAT_PCM).
    pub format_tag: U16<LittleEndian>,
    /// Number of channels: 1 = mono, 2 = stereo.
    pub channels: U16<LittleEndian>,
    /// Sampling frequency in Hz (typically 11025, 22050, 44100).
    pub sample_rate: U32<LittleEndian>,
    /// Average bytes per second (`sample_rate * block_align`).
    pub avg_bytes_per_sec: U32<LittleEndian>,
    /// Block alignment in bytes (`channels * bits_per_sample / 8`).
    pub block_align: U16<LittleEndian>,
    /// Bit depth per sample (8 or 16).
    pub bits_per_sample: U16<LittleEndian>,
}

/// Audio channel configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioChannels {
    /// Mono (1 channel).
    Mono,
    /// Stereo (2 channels: left, right).
    Stereo,
}

impl AudioChannels {
    /// Returns the integer channel count.
    #[must_use]
    pub const fn count(self) -> u16 {
        match self {
            Self::Mono => 1,
            Self::Stereo => 2,
        }
    }
}

/// Sample bit depth representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    /// 8-bit unsigned PCM sample (centered around 128 in WAV container).
    U8,
    /// 16-bit signed little-endian PCM sample.
    I16,
}

/// Decoded Sound Sample data buffer.
#[derive(Debug, Clone, PartialEq)]
pub enum PcmData {
    /// 8-bit PCM samples.
    U8(Vec<u8>),
    /// 16-bit signed PCM samples.
    I16(Vec<i16>),
}

/// Decoded WAV sound representation.
#[derive(Debug, Clone, PartialEq)]
pub struct WavSound {
    /// Audio channel layout.
    pub channels: AudioChannels,
    /// Sampling rate in Hz (e.g. 11025, 22050, 44100).
    pub sample_rate: u32,
    /// Sample format (8-bit or 16-bit).
    pub format: SampleFormat,
    /// Decoded PCM audio payload.
    pub data: PcmData,
    /// Total number of sample frames (per channel).
    pub sample_count: usize,
    /// Optional loop point sample index parsed from `cue ` chunk.
    pub loop_start: Option<u32>,
}

impl WavSound {
    /// Returns duration in seconds.
    #[must_use]
    pub fn duration_seconds(&self) -> f32 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.sample_count as f32 / self.sample_rate as f32
        }
    }
}

/// Validates whether a sample rate is a standard GoldSrc frequency (11025, 22050, 44100 Hz).
#[must_use]
pub const fn is_supported_sample_rate(rate: u32) -> bool {
    rate == 11025 || rate == 22050 || rate == 44100
}

struct ParsedChunks<'a> {
    fmt: WaveFmtPcm,
    data_bytes: &'a [u8],
    loop_start: Option<u32>,
}

fn parse_riff_chunks(buffer: &[u8]) -> Result<ParsedChunks<'_>, AudioError> {
    let mut cursor = 12;
    let mut fmt_header: Option<WaveFmtPcm> = None;
    let mut data_slice: Option<&[u8]> = None;
    let mut loop_start: Option<u32> = None;

    while cursor + 8 <= buffer.len() {
        let chunk_id = [
            buffer[cursor],
            buffer[cursor + 1],
            buffer[cursor + 2],
            buffer[cursor + 3],
        ];
        let chunk_size = u32::from_le_bytes([
            buffer[cursor + 4],
            buffer[cursor + 5],
            buffer[cursor + 6],
            buffer[cursor + 7],
        ]) as usize;

        let payload_start = cursor + 8;
        let payload_end = (payload_start + chunk_size).min(buffer.len());

        match &chunk_id {
            b"fmt " => {
                let fmt_bytes = &buffer[payload_start..payload_end];
                if fmt_bytes.len() < 16 {
                    return Err(AudioError::BufferTooSmall {
                        expected: 16,
                        got: fmt_bytes.len(),
                    });
                }
                let (fmt, _) = WaveFmtPcm::read_from_prefix(fmt_bytes).map_err(|_| {
                    AudioError::BufferTooSmall {
                        expected: 16,
                        got: fmt_bytes.len(),
                    }
                })?;
                fmt_header = Some(fmt);
            }
            b"data" => {
                data_slice = Some(&buffer[payload_start..payload_end]);
            }
            b"cue " => {
                let cue_bytes = &buffer[payload_start..payload_end];
                if cue_bytes.len() >= 36 {
                    let sample_offset = u32::from_le_bytes([
                        cue_bytes[32],
                        cue_bytes[33],
                        cue_bytes[34],
                        cue_bytes[35],
                    ]);
                    loop_start = Some(sample_offset);
                }
            }
            _ => {}
        }

        cursor = payload_start + chunk_size;
        if chunk_size % 2 == 1 {
            cursor += 1;
        }
    }

    let fmt = fmt_header.ok_or(AudioError::MissingChunk("fmt "))?;
    let data_bytes = data_slice.ok_or(AudioError::MissingChunk("data"))?;

    Ok(ParsedChunks {
        fmt,
        data_bytes,
        loop_start,
    })
}

/// Parses and decodes an in-memory RIFF/WAVE PCM audio buffer.
pub fn decode_wav(buffer: &[u8]) -> Result<WavSound, AudioError> {
    if buffer.len() < 12 {
        return Err(AudioError::BufferTooSmall {
            expected: 12,
            got: buffer.len(),
        });
    }

    let (riff_header, rest) =
        RiffChunkHeader::read_from_prefix(buffer).map_err(|_| AudioError::BufferTooSmall {
            expected: 8,
            got: buffer.len(),
        })?;

    if &riff_header.id != b"RIFF" {
        return Err(AudioError::InvalidMagic {
            location: "RIFF header",
            expected: *b"RIFF",
            got: riff_header.id,
        });
    }

    if rest.len() < 4 || &rest[0..4] != b"WAVE" {
        let mut got = [0u8; 4];
        if rest.len() >= 4 {
            got.copy_from_slice(&rest[0..4]);
        }
        return Err(AudioError::InvalidMagic {
            location: "WAVE header",
            expected: *b"WAVE",
            got,
        });
    }

    let parsed = parse_riff_chunks(buffer)?;
    let format_tag = parsed.fmt.format_tag.get();
    if format_tag != 1 {
        return Err(AudioError::UnsupportedFormat(format_tag));
    }

    let channels_count = parsed.fmt.channels.get();
    let channels = match channels_count {
        1 => AudioChannels::Mono,
        2 => AudioChannels::Stereo,
        other => return Err(AudioError::UnsupportedChannels(other)),
    };

    let sample_rate = parsed.fmt.sample_rate.get();
    if !is_supported_sample_rate(sample_rate) {
        return Err(AudioError::UnsupportedSampleRate(sample_rate));
    }

    let bits_per_sample = parsed.fmt.bits_per_sample.get();
    let bytes_per_sample = match bits_per_sample {
        8 => 1,
        16 => 2,
        other => return Err(AudioError::UnsupportedBitsPerSample(other)),
    };

    let block_align = (channels_count as usize) * bytes_per_sample;
    let sample_count = parsed.data_bytes.len() / block_align;
    let aligned_data_len = sample_count * block_align;
    let valid_data = &parsed.data_bytes[..aligned_data_len];

    let (format, pcm_data) = if bits_per_sample == 8 {
        (SampleFormat::U8, PcmData::U8(valid_data.to_vec()))
    } else {
        let (chunks, _) = valid_data.as_chunks::<2>();
        let mut samples = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            samples.push(i16::from_le_bytes(*chunk));
        }
        (SampleFormat::I16, PcmData::I16(samples))
    };

    let validated_loop = parsed.loop_start.filter(|&ls| (ls as usize) < sample_count);

    Ok(WavSound {
        channels,
        sample_rate,
        format,
        data: pcm_data,
        sample_count,
        loop_start: validated_loop,
    })
}

/// Helper function to create a valid in-memory PCM WAV file buffer for tests and asset synthesis.
#[must_use]
pub fn encode_wav_pcm(
    channels: AudioChannels,
    sample_rate: u32,
    bits_per_sample: u16,
    samples_raw: &[u8],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(44 + samples_raw.len());
    let channels_count = channels.count();
    let block_align = channels_count * (bits_per_sample / 8);
    let avg_bytes_per_sec = sample_rate * u32::from(block_align);
    let data_len = samples_raw.len() as u32;
    let riff_size = 36 + data_len;

    // RIFF chunk
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&riff_size.to_le_bytes());
    out.extend_from_slice(b"WAVE");

    // 'fmt ' chunk
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // subchunk1 size (16 for PCM)
    out.extend_from_slice(&1u16.to_le_bytes()); // format tag 1
    out.extend_from_slice(&channels_count.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&avg_bytes_per_sec.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());

    // 'data' chunk
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    out.extend_from_slice(samples_raw);

    out
}
