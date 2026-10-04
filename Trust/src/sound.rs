//! Redeem sounds (start, impact, contact, Lua scripts) play through OBS as
//! the Throwables Layer's own audio: they show on its fader in OBS's Audio
//! Mixer, and its volume, monitoring and tracks are set there like any other
//! source's. The sounds are decoded here (Media Foundation: MP3, WAV, AAC…;
//! lewton: OGG Vorbis), mixed (overlapping sounds add up) as 48 kHz stereo
//! float, and handed to OBS in 10 ms packets.
//!
//! The Throwables Layer belongs to the OBS plugin copy of Theater Scene. The
//! copy a Lua script loads (theater_chat.lua: Reader redeems) has none, so it
//! hands its sounds over with the `theater_scene_play_sound` OBS procedure.
//!
//! Before: on Windows the sounds went to a PowerShell `Media.SoundPlayer`
//! (WAV only: OGG/MP3 redeem sounds were silent), then to a hidden OBS media
//! source (in the stream but in no mixer, so not heard), then to waveOut
//! (Windows' "preferred" device, not OBS).

use std::ffi::{CStr, c_void};
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use crate::obs_source::obs_symbol;

type P = *mut c_void;

struct Fns {
    source_output_audio: unsafe extern "C" fn(P, *const SourceAudio),
    source_get_weak_source: unsafe extern "C" fn(P) -> P,
    weak_source_get_source: unsafe extern "C" fn(P) -> P,
    weak_source_release: unsafe extern "C" fn(P),
    source_release: unsafe extern "C" fn(P),
    os_gettime_ns: unsafe extern "C" fn() -> u64,
}

// Function pointers into libobs, which outlives us.
unsafe impl Send for Fns {}
unsafe impl Sync for Fns {}

fn fns() -> Option<&'static Fns> {
    static FNS: OnceLock<Option<Fns>> = OnceLock::new();
    FNS.get_or_init(|| {
        fn sym<T>(name: &CStr) -> Option<T> {
            let p = obs_symbol(name);
            // SAFETY: T is the function pointer type of the libobs export `name`.
            (!p.is_null()).then(|| unsafe { std::mem::transmute_copy(&p) })
        }
        Some(Fns {
            source_output_audio: sym(c"obs_source_output_audio")?,
            source_get_weak_source: sym(c"obs_source_get_weak_source")?,
            weak_source_get_source: sym(c"obs_weak_source_get_source")?,
            weak_source_release: sym(c"obs_weak_source_release")?,
            source_release: sym(c"obs_source_release")?,
            os_gettime_ns: sym(c"os_gettime_ns")?,
        })
    })
    .as_ref()
}

/// libobs's struct obs_source_audio.
#[repr(C)]
struct SourceAudio {
    data: [*const u8; 8],
    frames: u32,
    speakers: i32,
    format: i32,
    samples_per_sec: u32,
    timestamp: u64,
}

const SPEAKERS_STEREO: i32 = 2;
const AUDIO_FORMAT_FLOAT: i32 = 4;

/// The mix's sample rate (OBS resamples it to its own if that differs).
const RATE: u32 = 48_000;
/// Frames per packet handed to OBS: 10 ms.
const PACKET: usize = 480;
/// How far ahead of real time packets are handed over.
const LEAD: Duration = Duration::from_millis(60);

// ─── The Throwables Layer ────────────────────────────────────────────────────

/// Throwables Layer sources this copy created (weak references, newest last).
static LAYERS: parking_lot::Mutex<Vec<usize>> = parking_lot::Mutex::new(Vec::new());

/// Called when OBS creates a Throwables Layer source.
pub fn layer_created(source: P) {
    let Some(fns) = fns() else { return };
    if source.is_null() {
        return;
    }
    let weak = unsafe { (fns.source_get_weak_source)(source) };
    if !weak.is_null() {
        LAYERS.lock().push(weak as usize);
    }
}

/// A live Throwables Layer (a strong reference: release it), dropping any
/// that are gone.
fn layer(fns: &Fns) -> Option<P> {
    let mut layers = LAYERS.lock();
    while let Some(&weak) = layers.last() {
        let source = unsafe { (fns.weak_source_get_source)(weak as P) };
        if !source.is_null() {
            return Some(source);
        }
        unsafe { (fns.weak_source_release)(weak as P) };
        layers.pop();
    }
    None
}

// ─── Decoding and the mix ────────────────────────────────────────────────────

/// A decoded sound: interleaved integer PCM as the decoder gave it.
pub struct Pcm {
    pub channels: u16,
    pub rate: u32,
    pub bits: u16,
    pub data: Vec<u8>,
}

/// The sound as the mix wants it: stereo float at RATE (mono doubled, extra
/// channels dropped), resampled linearly — plenty for sound effects.
fn to_mix_format(pcm: &Pcm) -> Vec<f32> {
    let channels = pcm.channels.max(1) as usize;
    let bytes = (pcm.bits / 8).max(1) as usize;
    let frames = pcm.data.len() / (channels * bytes);
    if frames == 0 || pcm.rate == 0 {
        return Vec::new();
    }
    let sample = |frame: usize, channel: usize| -> f32 {
        let i = (frame * channels + channel.min(channels - 1)) * bytes;
        let b = &pcm.data[i..i + bytes];
        match bytes {
            1 => (b[0] as f32 - 128.0) / 128.0,
            2 => i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0,
            3 => (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0,
            _ => i32::from_le_bytes([b[0], b[1], b[2], b[3]]) as f32 / 2_147_483_648.0,
        }
    };
    let out_frames = (frames as u64 * RATE as u64 / pcm.rate as u64) as usize;
    let step = pcm.rate as f64 / RATE as f64;
    let mut out = Vec::with_capacity(out_frames * 2);
    for o in 0..out_frames {
        let pos = o as f64 * step;
        let i = (pos as usize).min(frames - 1);
        let next = (i + 1).min(frames - 1);
        let t = (pos - i as f64) as f32;
        for channel in 0..2 {
            let (a, b) = (sample(i, channel), sample(next, channel));
            out.push(a + (b - a) * t);
        }
    }
    out
}

struct Voice {
    /// Stereo, interleaved, at RATE.
    samples: Vec<f32>,
    /// Next frame to mix.
    frame: usize,
    volume: f32,
}

static VOICES: parking_lot::Mutex<Vec<Voice>> = parking_lot::Mutex::new(Vec::new());
/// A mixer thread is running (set and cleared under the VOICES lock).
static MIXING: AtomicBool = AtomicBool::new(false);

/// Adds `voice` to the mix, starting the mixer if it isn't running.
fn add_voice(voice: Voice) {
    let mut voices = VOICES.lock();
    voices.push(voice);
    if !MIXING.swap(true, Ordering::SeqCst) {
        std::thread::spawn(mixer);
    }
}

/// Mixes packets of the playing sounds into one and hands each to the
/// Throwables Layer, paced by the clock, until nothing is left playing.
fn mixer() {
    let Some(fns) = fns() else {
        VOICES.lock().clear();
        MIXING.store(false, Ordering::SeqCst);
        return;
    };
    let started = Instant::now();
    let base_ts = unsafe { (fns.os_gettime_ns)() };
    let mut sent: u64 = 0;
    let mut packet = vec![0f32; PACKET * 2];
    loop {
        let played = Duration::from_nanos(sent * 1_000_000_000 / RATE as u64);
        let now = started.elapsed();
        if played > now + LEAD {
            std::thread::sleep((played - now - LEAD).min(Duration::from_millis(10)));
            continue;
        }
        {
            let mut voices = VOICES.lock();
            if voices.is_empty() {
                MIXING.store(false, Ordering::SeqCst);
                return;
            }
            packet.fill(0.0);
            for voice in voices.iter_mut() {
                let start = voice.frame * 2;
                let end = (start + PACKET * 2).min(voice.samples.len());
                for (out, s) in packet.iter_mut().zip(&voice.samples[start..end]) {
                    *out += s * voice.volume;
                }
                voice.frame += PACKET;
            }
            voices.retain(|v| v.frame * 2 < v.samples.len());
        }
        for s in &mut packet {
            *s = s.clamp(-1.0, 1.0);
        }
        if let Some(source) = layer(fns) {
            let audio = SourceAudio {
                data: [packet.as_ptr() as *const u8, std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null()],
                frames: PACKET as u32,
                speakers: SPEAKERS_STEREO,
                format: AUDIO_FORMAT_FLOAT,
                samples_per_sec: RATE,
                timestamp: base_ts + sent * 1_000_000_000 / RATE as u64,
            };
            unsafe {
                (fns.source_output_audio)(source, &audio);
                (fns.source_release)(source);
            }
        }
        sent += PACKET as u64;
    }
}

/// Decodes `path` and adds it to the Throwables Layer's mix at `volume`.
/// Err when there's no Throwables Layer in this copy or it can't be decoded.
/// (Also what the `theater_scene_play_sound` procedure runs.)
pub fn queue_file(path: &Path, volume: f32) -> Result<(), String> {
    let fns = fns().ok_or("OBS isn't there")?;
    match layer(fns) {
        Some(source) => unsafe { (fns.source_release)(source) },
        None => return Err("no Throwables Layer source (add it to a scene to hear redeem sounds)".into()),
    }
    let pcm = decode(path)?;
    let samples = to_mix_format(&pcm);
    if !samples.is_empty() {
        add_voice(Voice { samples, frame: 0, volume: volume.max(0.0) });
    }
    Ok(())
}

/// Plays the sound file `path` through the Throwables Layer at `volume` (1.0 =
/// as recorded); `delete_after`: remove the file once it's decoded (a
/// temporary copy). Returns at once: the sound is decoded and handed over on
/// a thread of its own, never on the caller's (sounds are played from inside
/// Theater's render and while its physics lock is held).
pub fn play_file(path: &Path, volume: f32, delete_after: bool) -> bool {
    let path = path.to_path_buf();
    std::thread::spawn(move || {
        let has_layer = fns().and_then(layer).map(|source| unsafe { (fns().unwrap().source_release)(source) }).is_some();
        let result = if has_layer {
            queue_file(&path, volume)
        } else {
            // The plugin copy has the Throwables Layer
            match crate::obs_procs::call_play_sound(&path, volume) {
                Some(Ok(())) => Ok(()),
                Some(Err(e)) => Err(e),
                None => queue_file(&path, volume),
            }
        };
        if let Err(e) = result {
            crate::obs_source::trust_log(&format!("[Trust Sound] ⚠️ '{}' not played: {}", path.display(), e));
        }
        if delete_after {
            let _ = std::fs::remove_file(&path);
        }
    });
    true
}

#[cfg(not(target_os = "windows"))]
fn decode(_path: &Path) -> Result<Pcm, String> {
    Err("decoding is Windows-only (Linux plays sounds with pw-play)".into())
}

#[cfg(target_os = "windows")]
fn decode(path: &Path) -> Result<Pcm, String> {
    decoder::decode(path).map_err(|e| e.to_string())
}

/// Media Foundation (MP3, WAV, AAC…) and lewton (OGG Vorbis) to PCM.
#[cfg(target_os = "windows")]
pub(crate) mod decoder {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows::Win32::Foundation::BOOL;
    use windows::Win32::Media::MediaFoundation::{
        IMFSample, MF_MT_AUDIO_BITS_PER_SAMPLE, MF_MT_AUDIO_NUM_CHANNELS, MF_MT_AUDIO_SAMPLES_PER_SECOND,
        MF_MT_MAJOR_TYPE, MF_MT_SUBTYPE, MF_SOURCE_READER_ALL_STREAMS, MF_SOURCE_READER_FIRST_AUDIO_STREAM,
        MF_SOURCE_READERF_ENDOFSTREAM, MF_VERSION, MFAudioFormat_PCM, MFCreateMediaType,
        MFCreateSourceReaderFromURL, MFMediaType_Audio, MFSTARTUP_FULL, MFShutdown, MFStartup,
    };
    use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
    use windows::core::PCWSTR;

    use super::Pcm;

    /// Media Foundation, started for the calling thread and stopped again when dropped.
    struct MediaFoundation;

    impl MediaFoundation {
        fn start() -> windows::core::Result<Self> {
            unsafe {
                CoInitializeEx(None, COINIT_MULTITHREADED).ok()?;
                if let Err(e) = MFStartup(MF_VERSION, MFSTARTUP_FULL) {
                    CoUninitialize();
                    return Err(e);
                }
            }
            Ok(MediaFoundation)
        }
    }

    impl Drop for MediaFoundation {
        fn drop(&mut self) {
            unsafe {
                let _ = MFShutdown();
                CoUninitialize();
            }
        }
    }

    fn failed(what: String) -> windows::core::Error {
        windows::core::Error::new(windows::core::HRESULT(0x8000_4005u32 as i32), what)
    }

    /// Whether the file is an Ogg stream (by its signature, not its name:
    /// a base64 sound's temporary copy may carry the wrong extension).
    fn is_ogg(path: &Path) -> bool {
        use std::io::Read;
        let mut magic = [0u8; 4];
        std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut magic)).is_ok() && &magic == b"OggS"
    }

    /// OGG Vorbis, with lewton: Media Foundation's Source Reader doesn't read
    /// Ogg here (MF_E_UNSUPPORTED_BYTESTREAM_TYPE, even with Web Media Extensions).
    fn decode_ogg(path: &Path) -> windows::core::Result<Pcm> {
        let file = std::fs::File::open(path).map_err(|e| failed(e.to_string()))?;
        let mut reader = lewton::inside_ogg::OggStreamReader::new(std::io::BufReader::new(file))
            .map_err(|e| failed(format!("Ogg Vorbis: {e}")))?;
        let channels = reader.ident_hdr.audio_channels as u16;
        let rate = reader.ident_hdr.audio_sample_rate;
        let mut data = Vec::new();
        while let Some(samples) = reader.read_dec_packet_itl().map_err(|e| failed(format!("Ogg Vorbis: {e}")))? {
            data.extend(samples.iter().flat_map(|s| s.to_le_bytes()));
        }
        Ok(Pcm { channels, rate, bits: 16, data })
    }

    /// The whole sound, decoded to PCM.
    pub fn decode(path: &Path) -> windows::core::Result<Pcm> {
        if is_ogg(path) {
            return decode_ogg(path);
        }
        let _mf = MediaFoundation::start()?;
        let url: Vec<u16> = path.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
        unsafe {
            let reader = MFCreateSourceReaderFromURL(PCWSTR(url.as_ptr()), None)?;
            let audio = MF_SOURCE_READER_FIRST_AUDIO_STREAM.0 as u32;
            reader.SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, BOOL::from(false))?;
            reader.SetStreamSelection(audio, BOOL::from(true))?;
            let wanted = MFCreateMediaType()?;
            wanted.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Audio)?;
            wanted.SetGUID(&MF_MT_SUBTYPE, &MFAudioFormat_PCM)?;
            reader.SetCurrentMediaType(audio, None, &wanted)?;
            let got = reader.GetCurrentMediaType(audio)?;
            let channels = got.GetUINT32(&MF_MT_AUDIO_NUM_CHANNELS)? as u16;
            let rate = got.GetUINT32(&MF_MT_AUDIO_SAMPLES_PER_SECOND)?;
            let bits = got.GetUINT32(&MF_MT_AUDIO_BITS_PER_SAMPLE)? as u16;

            let mut data = Vec::new();
            loop {
                let mut flags = 0u32;
                let mut sample: Option<IMFSample> = None;
                reader.ReadSample(audio, 0, None, Some(&mut flags), None, Some(&mut sample))?;
                if let Some(sample) = sample {
                    let buffer = sample.ConvertToContiguousBuffer()?;
                    let mut ptr: *mut u8 = std::ptr::null_mut();
                    let mut len = 0u32;
                    buffer.Lock(&mut ptr, None, Some(&mut len))?;
                    if !ptr.is_null() {
                        data.extend_from_slice(std::slice::from_raw_parts(ptr, len as usize));
                    }
                    buffer.Unlock()?;
                }
                if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                    break;
                }
            }
            Ok(Pcm { channels, rate, bits, data })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pcm16(channels: u16, rate: u32, samples: &[i16]) -> Pcm {
        Pcm { channels, rate, bits: 16, data: samples.iter().flat_map(|s| s.to_le_bytes()).collect() }
    }

    #[test]
    fn mono_is_doubled_to_stereo_at_the_mix_rate() {
        let out = to_mix_format(&pcm16(1, RATE, &[16384, -16384]));
        assert_eq!(out, vec![0.5, 0.5, -0.5, -0.5]);
    }

    #[test]
    fn other_rates_are_resampled_to_the_mix_rate() {
        // 24 kHz stereo, 100 frames → 200 frames at 48 kHz, halfway values interpolated
        let samples: Vec<i16> = (0..100).flat_map(|i| [i as i16 * 100, -(i as i16) * 100]).collect();
        let out = to_mix_format(&pcm16(2, 24_000, &samples));
        assert_eq!(out.len(), 200 * 2);
        assert!((out[2] - 50.0 / 32768.0).abs() < 1e-6, "left, between frames 0 and 1: {}", out[2]);
        assert!((out[3] + 50.0 / 32768.0).abs() < 1e-6, "right: {}", out[3]);
        assert!((out[4] - 100.0 / 32768.0).abs() < 1e-6);
    }

    #[test]
    fn extra_channels_are_dropped_and_other_depths_read() {
        // 3 channels, 8-bit: L, R kept
        let out = to_mix_format(&Pcm { channels: 3, rate: RATE, bits: 8, data: vec![192, 64, 0] });
        assert_eq!(out, vec![0.5, -0.5]);
        // Empty or broken input gives nothing, not a panic
        assert!(to_mix_format(&Pcm { channels: 2, rate: RATE, bits: 16, data: vec![1] }).is_empty());
        assert!(to_mix_format(&Pcm { channels: 2, rate: 0, bits: 16, data: vec![0; 8] }).is_empty());
    }

    #[test]
    fn a_sound_is_not_played_without_a_throwables_layer() {
        // No OBS here: no layer, nothing queued, and it says why
        assert!(queue_file(Path::new("nothing.ogg"), 1.0).is_err());
    }

    /// Decodes the redeems' own sounds (when this PC has them): MP3 with Media
    /// Foundation, OGG with lewton.
    #[cfg(target_os = "windows")]
    #[test]
    fn redeem_sounds_decode() {
        let Some(home) = std::env::var_os("USERPROFILE") else { return };
        let dir = Path::new(&home).join(r"OneDrive\Desktop\obs");
        for name in ["Nomnomnom.mp3", "NomoftheValkery.ogg"] {
            let path = dir.join(name);
            if !path.is_file() {
                continue;
            }
            let pcm = decoder::decode(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
            let mixed = to_mix_format(&pcm);
            let seconds = mixed.len() as f64 / 2.0 / RATE as f64;
            assert!(seconds > 0.1, "{name}: only {seconds:.2}s decoded");
            assert!(mixed.iter().any(|s| s.abs() > 0.01), "{name}: silent");
        }
    }
}
