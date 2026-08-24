//! AudioClip memory/decoder/composite reader family.
//!
//! The boundaries mirror AudioClip::read at sub_100571DD8, the looping
//! instance wrapper at sub_100571E54, and composite reading at sub_1005792B0.

use std::{fs, io::Cursor, sync::Arc};

use anyhow::{Context, Result, bail};
use rodio::{Decoder, Source};
use stella_script::{AudioAssetSource, mpg123_compat::Mpg123Compatibility};

pub(crate) struct NativeClip {
    pub(crate) reader: NativeReader,
    pub(crate) channels: u16,
    pub(crate) bits_per_sample: u16,
    #[allow(dead_code)]
    pub(crate) sample_rate: u32,
}

impl NativeClip {
    pub(crate) fn empty() -> Self {
        Self {
            reader: NativeReader::Memory {
                bytes: Arc::from([]),
                cursor: 0,
            },
            channels: 0,
            bits_per_sample: 0,
            sample_rate: 0,
        }
    }
}

pub(crate) enum NativeReader {
    Memory {
        bytes: Arc<[u8]>,
        cursor: usize,
    },
    Decoder(StreamingDecoder),
    Sequence {
        parts: Vec<NativeClip>,
        index: usize,
    },
}

impl NativeReader {
    pub(super) fn read_into(&mut self, output: &mut Vec<u8>, requested: usize) -> usize {
        if requested == 0 {
            return 0;
        }
        match self {
            Self::Memory { bytes, cursor } => {
                let copied = requested.min(bytes.len().saturating_sub(*cursor));
                output.extend_from_slice(&bytes[*cursor..*cursor + copied]);
                *cursor += copied;
                copied
            }
            Self::Decoder(decoder) => decoder.read_into(output, requested),
            Self::Sequence { parts, index } => {
                let start = output.len();
                loop {
                    let Some(part) = parts.get_mut(*index) else {
                        return output.len() - start;
                    };
                    let remaining = requested - (output.len() - start);
                    let read = part.reader.read_into(output, remaining);
                    if read == 0 {
                        if *index + 1 < parts.len() {
                            // Composite state shares the cursor at +0x18 and
                            // stores its child index at +0x1C. The child switch
                            // clears the former and advances the latter, then
                            // returns without reading the next child yet.
                            *index += 1;
                            parts[*index].reader.reset();
                        }
                        return output.len() - start;
                    }
                    if output.len() - start >= requested {
                        return requested;
                    }
                }
            }
        }
    }

    /// Reset the per-instance reader state. Returns false only for a source
    /// which cannot ever produce a byte, preventing the target's intentional
    /// empty-loop spin from hanging the cross-platform callback.
    pub(super) fn reset(&mut self) -> bool {
        match self {
            Self::Memory { bytes, cursor } => {
                *cursor = 0;
                !bytes.is_empty()
            }
            Self::Decoder(decoder) => decoder.reset(),
            Self::Sequence { parts, index } => {
                *index = 0;
                for part in parts.iter_mut() {
                    part.reader.reset();
                }
                parts.first().is_some_and(|part| part.reader.can_produce())
            }
        }
    }

    fn can_produce(&self) -> bool {
        match self {
            Self::Memory { bytes, .. } => !bytes.is_empty(),
            Self::Decoder(decoder) => decoder.valid,
            Self::Sequence { parts, .. } => parts.iter().any(|part| part.reader.can_produce()),
        }
    }
}

pub(crate) struct StreamingDecoder {
    encoded: Arc<[u8]>,
    hint: String,
    decoder: Decoder<Cursor<Arc<[u8]>>>,
    bits_per_sample: u16,
    mpg123_compatibility: Option<Mpg123Compatibility>,
    pending: [u8; 4],
    pending_start: usize,
    pending_end: usize,
    valid: bool,
}

impl StreamingDecoder {
    fn new(encoded: Arc<[u8]>, hint: String, bits_per_sample: u16) -> Result<Self> {
        let decoder = make_decoder(Arc::clone(&encoded), &hint)?;
        let mpg123_compatibility = (bits_per_sample == 16 && hint.eq_ignore_ascii_case("mp3"))
            .then(|| Mpg123Compatibility::new(&encoded));
        Ok(Self {
            encoded,
            hint,
            decoder,
            bits_per_sample,
            mpg123_compatibility,
            pending: [0; 4],
            pending_start: 0,
            pending_end: 0,
            valid: true,
        })
    }

    fn read_into(&mut self, output: &mut Vec<u8>, requested: usize) -> usize {
        let start = output.len();
        while output.len() - start < requested {
            if self.pending_start < self.pending_end {
                let copied =
                    (requested - (output.len() - start)).min(self.pending_end - self.pending_start);
                output.extend_from_slice(
                    &self.pending[self.pending_start..self.pending_start + copied],
                );
                self.pending_start += copied;
                continue;
            }
            let mpg123_sample = if let Some(compatibility) = self.mpg123_compatibility.as_mut() {
                let Some(sample) = compatibility.next_sample(&mut self.decoder) else {
                    break;
                };
                Some(sample)
            } else {
                None
            };
            let sample = if mpg123_sample.is_none() {
                let Some(sample) = self.decoder.next() else {
                    break;
                };
                sample
            } else {
                0.0
            };
            self.pending_start = 0;
            self.pending_end = match self.bits_per_sample {
                8 => {
                    self.pending[0] = ((sample * 128.0) + 128.0)
                        .round_ties_even()
                        .clamp(0.0, 255.0) as u8;
                    1
                }
                16 => {
                    let sample = mpg123_sample
                        .unwrap_or_else(|| (sample * 32_768.0).round_ties_even() as i16);
                    self.pending[..2].copy_from_slice(&sample.to_le_bytes());
                    2
                }
                32 => {
                    self.pending.copy_from_slice(
                        &((sample * 2_147_483_648.0).round_ties_even() as i32).to_le_bytes(),
                    );
                    4
                }
                _ => {
                    self.valid = false;
                    break;
                }
            };
        }
        output.len() - start
    }

    fn reset(&mut self) -> bool {
        match make_decoder(Arc::clone(&self.encoded), &self.hint) {
            Ok(decoder) => {
                self.decoder = decoder;
                self.mpg123_compatibility = (self.bits_per_sample == 16
                    && self.hint.eq_ignore_ascii_case("mp3"))
                .then(|| Mpg123Compatibility::new(&self.encoded));
                self.pending_start = 0;
                self.pending_end = 0;
                self.valid = true;
                true
            }
            Err(_) => {
                self.valid = false;
                false
            }
        }
    }
}

fn make_decoder(encoded: Arc<[u8]>, hint: &str) -> Result<Decoder<Cursor<Arc<[u8]>>>> {
    let byte_len = encoded.len() as u64;
    Decoder::builder()
        .with_data(Cursor::new(encoded))
        .with_byte_len(byte_len)
        .with_hint(hint)
        .with_gapless(true)
        .build()
        .context("construct retained audio decoder")
}

pub(crate) fn decode_asset(source: &AudioAssetSource) -> Result<NativeClip> {
    match source {
        AudioAssetSource::File(path) => {
            let encoded = Arc::<[u8]>::from(
                fs::read(path).with_context(|| format!("open {}", path.display()))?,
            );
            let decoder = make_decoder(Arc::clone(&encoded), extension_hint(path))?;
            let channels = decoder.channels().get();
            let sample_rate = decoder.sample_rate().get();
            drop(decoder);
            Ok(NativeClip {
                reader: NativeReader::Decoder(StreamingDecoder::new(
                    encoded,
                    extension_hint(path).to_owned(),
                    16,
                )?),
                channels,
                bits_per_sample: 16,
                sample_rate,
            })
        }
        AudioAssetSource::EncodedFile {
            path,
            data,
            channels,
            bits_per_sample,
            sample_rate,
            ..
        } => Ok(NativeClip {
            reader: NativeReader::Decoder(StreamingDecoder::new(
                Arc::clone(data),
                extension_hint(path).to_owned(),
                *bits_per_sample,
            )?),
            channels: *channels,
            bits_per_sample: *bits_per_sample,
            sample_rate: *sample_rate,
        }),
        AudioAssetSource::PcmData {
            data,
            channels,
            bits_per_sample,
            sample_rate,
            ..
        }
        | AudioAssetSource::RawPcmFile {
            data,
            channels,
            bits_per_sample,
            sample_rate,
            ..
        } => Ok(NativeClip {
            reader: NativeReader::Memory {
                bytes: Arc::clone(data),
                cursor: 0,
            },
            channels: *channels,
            bits_per_sample: *bits_per_sample,
            sample_rate: *sample_rate,
        }),
        AudioAssetSource::Sequence(parts) => {
            let mut parts = parts.iter();
            let Some(first) = parts.next() else {
                bail!("empty composite audio clip")
            };
            let first = decode_asset(first)?;
            let channels = first.channels;
            let bits_per_sample = first.bits_per_sample;
            let sample_rate = first.sample_rate;
            let mut decoded = vec![first];
            for part in parts {
                decoded.push(decode_asset(part)?);
            }
            Ok(NativeClip {
                reader: NativeReader::Sequence {
                    parts: decoded,
                    index: 0,
                },
                channels,
                bits_per_sample,
                sample_rate,
            })
        }
    }
}

fn extension_hint(path: &std::path::Path) -> &str {
    path.extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("")
}
