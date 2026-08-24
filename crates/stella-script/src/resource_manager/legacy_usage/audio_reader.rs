//! AudioReader type detection, decoder construction and resident PCM accounting.

use std::{fs, io::Cursor, path::Path, sync::Arc, time::Duration};

use rodio::{Decoder, Source};

use crate::{
    AudioAssetSource, app_data_path, mpg123_compat::Mpg123Compatibility, resolve_data_file,
};

mod mp3;
mod ogg;

#[cfg(test)]
pub(super) fn audio_duration(path: &Path) -> Option<Duration> {
    audio_file_info(path, false)?.duration
}

/// Decoder state which can be retained even when a malformed-but-accepted
/// zero-length WAV has no meaningful sample rate from which to derive time.
pub(in crate::resource_manager) struct AudioFileInfo {
    pub(in crate::resource_manager) source: AudioAssetSource,
    pub(in crate::resource_manager) duration: Option<Duration>,
    pub(in crate::resource_manager) sample_frames: Option<u64>,
    pub(in crate::resource_manager) resident_bytes: Option<u32>,
}

pub(in crate::resource_manager) fn audio_file_info(
    path: &Path,
    streaming: bool,
) -> Option<AudioFileInfo> {
    let bytes = fs::read(path).ok()?;
    let file_type = native_file_type(path, &bytes);
    let data = Arc::<[u8]>::from(bytes.clone());
    let raw_stream = Some(AudioStreamInfo {
        samples: bytes.len() as u64 / 4,
        sample_rate: 44_100,
    });
    let (source, stream, resident_bytes) = match file_type {
        NativeFileType::Raw if streaming => (
            AudioAssetSource::RawPcmFile {
                path: path.to_path_buf(),
                data,
                streaming,
                channels: 2,
                bits_per_sample: 16,
                sample_rate: 44_100,
            },
            raw_stream,
            resident_byte_count(bytes.len()),
        ),
        NativeFileType::Raw => (
            AudioAssetSource::PcmData {
                origin: path.to_path_buf(),
                data,
                channels: 2,
                bits_per_sample: 16,
                sample_rate: 44_100,
            },
            raw_stream,
            resident_byte_count(bytes.len()),
        ),
        NativeFileType::Wav => {
            let wav = wav_reader_info(&bytes)?;
            let resident_bytes = wav.data_bytes.filter(|bytes| *bytes > 0);
            let source = if streaming {
                AudioAssetSource::EncodedFile {
                    path: path.to_path_buf(),
                    data,
                    streaming,
                    channels: wav.channels,
                    bits_per_sample: wav.bits_per_sample,
                    sample_rate: wav.sample_rate,
                }
            } else {
                AudioAssetSource::PcmData {
                    origin: path.to_path_buf(),
                    data: wav_pcm_data(&bytes, wav)?,
                    channels: wav.channels,
                    bits_per_sample: wav.bits_per_sample,
                    sample_rate: wav.sample_rate,
                }
            };
            (source, wav.stream, resident_bytes)
        }
        NativeFileType::Mp3 if streaming => {
            let (channels, sample_rate) = compressed_stream_format(Arc::clone(&data), "mp3")?;
            let stream = Mpg123Compatibility::new(&data)
                .target_interleaved_samples()
                .map(|samples| AudioStreamInfo {
                    samples: u64::from(samples) / u64::from(channels),
                    sample_rate,
                })
                .or_else(|| mp3::stream_info(&bytes));
            (
                AudioAssetSource::EncodedFile {
                    path: path.to_path_buf(),
                    data,
                    streaming,
                    channels,
                    bits_per_sample: 16,
                    sample_rate,
                },
                Some(stream?),
                None,
            )
        }
        NativeFileType::Mp3 => decoded_compressed_clip(path, data, "mp3")?,
        NativeFileType::Ogg if streaming => {
            let (channels, sample_rate) = compressed_stream_format(Arc::clone(&data), "ogg")?;
            (
                AudioAssetSource::EncodedFile {
                    path: path.to_path_buf(),
                    data,
                    streaming,
                    channels,
                    bits_per_sample: 16,
                    sample_rate,
                },
                Some(ogg::stream_info(&bytes)?),
                None,
            )
        }
        NativeFileType::Ogg => decoded_compressed_clip(path, data, "ogg")?,
        NativeFileType::Unsupported => return None,
    };
    Some(AudioFileInfo {
        source,
        duration: stream.and_then(stream_duration),
        sample_frames: stream.map(|stream| stream.samples),
        resident_bytes,
    })
}

/// `sub_10045A1C8` drains every non-streaming decoder into a byte vector before
/// constructing the clip. Symphonia supplies the cross-platform MPEG/Vorbis
/// synthesis; Purple's mpg123/vorbisfile readers expose signed 16-bit PCM.
fn decoded_compressed_clip(
    path: &Path,
    encoded: Arc<[u8]>,
    hint: &str,
) -> Option<(AudioAssetSource, Option<AudioStreamInfo>, Option<u32>)> {
    let mut mpg123 = (hint == "mp3").then(|| Mpg123Compatibility::new(&encoded));
    let decoder = compressed_decoder(encoded, hint)?;
    let channels = decoder.channels().get();
    let sample_rate = decoder.sample_rate().get();
    let mut pcm = Vec::new();
    let (minimum_samples, _) = decoder.size_hint();
    pcm.try_reserve_exact(minimum_samples.checked_mul(2)?)
        .ok()?;
    let mut decoder = decoder;
    loop {
        let quantized = if let Some(compatibility) = mpg123.as_mut() {
            compatibility.next_sample(&mut decoder)
        } else {
            decoder
                .next()
                .map(|sample| (sample * 32_768.0).round_ties_even() as i16)
        };
        let Some(quantized) = quantized else {
            break;
        };
        pcm.extend_from_slice(&quantized.to_le_bytes());
    }
    let samples = (pcm.len() as u64 / 2) / u64::from(channels);
    let stream = Some(AudioStreamInfo {
        samples,
        sample_rate,
    });
    let resident_bytes = resident_byte_count(pcm.len());
    Some((
        AudioAssetSource::PcmData {
            origin: path.to_path_buf(),
            data: Arc::from(pcm),
            channels,
            bits_per_sample: 16,
            sample_rate,
        },
        stream,
        resident_bytes,
    ))
}

fn compressed_stream_format(encoded: Arc<[u8]>, hint: &str) -> Option<(u16, u32)> {
    let decoder = compressed_decoder(encoded, hint)?;
    Some((decoder.channels().get(), decoder.sample_rate().get()))
}

fn compressed_decoder(encoded: Arc<[u8]>, hint: &str) -> Option<Decoder<Cursor<Arc<[u8]>>>> {
    let byte_len = encoded.len() as u64;
    Decoder::builder()
        .with_data(Cursor::new(encoded))
        .with_byte_len(byte_len)
        .with_hint(hint)
        .with_gapless(true)
        .build()
        .ok()
}

fn resident_byte_count(bytes: usize) -> Option<u32> {
    u32::try_from(bytes).ok().filter(|bytes| *bytes > 0)
}

fn stream_duration(stream: AudioStreamInfo) -> Option<Duration> {
    if stream.sample_rate == 0 {
        return None;
    }
    let whole_seconds = stream.samples / u64::from(stream.sample_rate);
    let remainder = stream.samples % u64::from(stream.sample_rate);
    let nanoseconds = remainder.checked_mul(1_000_000_000)? / u64::from(stream.sample_rate);
    Some(Duration::new(whole_seconds, nanoseconds as u32))
}

pub(in crate::resource_manager) fn audio_file_path(
    data_root: &Path,
    requested: &str,
    from_app_data: bool,
) -> Option<std::path::PathBuf> {
    if from_app_data {
        return app_data_path(data_root, requested)
            .ok()
            .filter(|path| path.is_file());
    }
    resolve_data_file(data_root, requested).ok().or_else(|| {
        let requested = requested.trim_start_matches('/');
        resolve_data_file(data_root, &format!("audio/{requested}")).ok()
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeFileType {
    Raw,
    Wav,
    Mp3,
    Ogg,
    Unsupported,
}

/// Mirrors `sub_1004FB774`. Its four-byte helper reverses the host word after
/// reading, so these constants are compared in file byte order. Ogg has no
/// magic branch in Purple 1.1.6 and is selected only by its extension.
fn native_file_type(path: &Path, bytes: &[u8]) -> NativeFileType {
    if let Some(first) = bytes
        .get(..4)
        .and_then(|word| <[u8; 4]>::try_from(word).ok())
        .map(u32::from_be_bytes)
    {
        let upper_16 = first & 0xffff_0000;
        let upper_24 = first & 0xffff_ff00;
        let image_magic = upper_16 == 0x424d_0000
            || upper_24 == 0xffd8_ff00
            || first == u32::from_be_bytes(*b"DDS ")
            || first == u32::from_be_bytes(*b"8BPS")
            || first == 0x8950_4e47
            || first == u32::from_be_bytes(*b"GIF8")
            || first == 0x4949_2a00
            || first == 0x4d4d_002a
            || (first >> 1) == 0x282b_2901
            || first == u32::from_be_bytes(*b"hgrf");
        if image_magic {
            return NativeFileType::Unsupported;
        }
        if upper_16 == 0xfffb_0000 || upper_24 == 0x4944_3300 {
            return NativeFileType::Mp3;
        }
        if first == u32::from_be_bytes(*b"RIFF")
            && let Some(form) = bytes.get(8..12)
        {
            if form == b"WAVE" {
                return NativeFileType::Wav;
            }
            if form == b"WEBP" {
                return NativeFileType::Unsupported;
            }
        }
    }

    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_uppercase)
        .as_deref()
    {
        Some("WAV") => NativeFileType::Wav,
        Some("MP3") => NativeFileType::Mp3,
        Some("OGG") => NativeFileType::Ogg,
        Some(
            "BMP" | "TGA" | "JPG" | "JPEG" | "DDS" | "PSD" | "PNG" | "PCX" | "PNM" | "GIF" | "TIF"
            | "TIFF" | "PVR" | "HGR" | "RAW" | "WEBP",
        ) => NativeFileType::Unsupported,
        _ => NativeFileType::Raw,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WavReaderInfo {
    stream: Option<AudioStreamInfo>,
    data_bytes: Option<u32>,
    data_offset: Option<usize>,
    channels: u16,
    bits_per_sample: u16,
    sample_rate: u32,
}

/// Reproduces the permissive RIFF walker in `sub_100578858`: the container
/// size is ignored, unknown chunks are not rounded to an even boundary, EOF
/// before any chunk is accepted, but a partial next chunk header is not.
fn wav_reader_info(bytes: &[u8]) -> Option<WavReaderInfo> {
    if bytes.len() < 12 || bytes.get(..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return None;
    }
    let mut cursor = 12usize;
    let mut format_seen = false;
    let mut channels = 0;
    let mut bits_per_sample = 0;
    let mut sample_rate = 0;
    let mut block_align = None;
    let mut data_bytes = None;
    let mut data_offset = None;
    while cursor < bytes.len() {
        if cursor.checked_add(8)? > bytes.len() {
            return None;
        }
        let chunk = bytes.get(cursor..cursor + 4)?;
        let size = u32::from_le_bytes(bytes.get(cursor + 4..cursor + 8)?.try_into().ok()?);
        let payload = cursor.checked_add(8)?;
        if chunk == b"fmt " && size >= 16 {
            if u16::from_le_bytes(bytes.get(payload..payload + 2)?.try_into().ok()?) != 1 {
                return None;
            }
            format_seen = true;
            channels = u16::from_le_bytes(bytes.get(payload + 2..payload + 4)?.try_into().ok()?);
            sample_rate = u32::from_le_bytes(bytes.get(payload + 4..payload + 8)?.try_into().ok()?);
            block_align = Some(u16::from_le_bytes(
                bytes.get(payload + 12..payload + 14)?.try_into().ok()?,
            ));
            bits_per_sample =
                u16::from_le_bytes(bytes.get(payload + 14..payload + 16)?.try_into().ok()?);
        } else if chunk == b"fmt " {
            // The native fixed stack buffer is subsequently read through
            // offset 15. Treat a short payload as deterministic failure rather
            // than reproducing undefined uninitialized-stack data.
            return None;
        } else if chunk == b"data" {
            if !format_seen {
                return None;
            }
            data_bytes = Some(size);
            data_offset = Some(payload);
            break;
        }
        cursor = payload.checked_add(size as usize)?;
    }
    let stream = block_align
        .filter(|align| sample_rate > 0 && *align > 0)
        .map(|block_align| AudioStreamInfo {
            samples: u64::from(data_bytes.unwrap_or(0)) / u64::from(block_align),
            sample_rate,
        });
    Some(WavReaderInfo {
        stream,
        data_bytes,
        data_offset,
        channels,
        bits_per_sample,
        sample_rate,
    })
}

fn wav_pcm_data(bytes: &[u8], wav: WavReaderInfo) -> Option<Arc<[u8]>> {
    let declared = wav.data_bytes.unwrap_or(0) as usize;
    let mut pcm = Vec::new();
    pcm.try_reserve_exact(declared).ok()?;
    pcm.resize(declared, 0);
    if let Some(offset) = wav.data_offset {
        let readable = declared.min(bytes.len().saturating_sub(offset));
        pcm.get_mut(..readable)?
            .copy_from_slice(bytes.get(offset..offset + readable)?);
    }
    Some(Arc::from(pcm))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AudioStreamInfo {
    /// Decoded sample frames per channel.
    samples: u64,
    sample_rate: u32,
}

#[cfg(test)]
mod tests;
