use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use rodio::decoder::DecoderError;
use rodio::{ChannelCount, Decoder, DeviceSinkBuilder, Sample, SampleRate, Source};

/// Commands sent from Tauri command handlers to the dedicated audio thread.
pub enum AudioCommand {
    Start,
    Stop,
}

/// The bell sound, embedded in the binary so no external file is needed at
/// runtime (works the same in `tauri dev` and in a bundled release build).
/// Pre-processed into a crossfade-looped WAV so looping it is sample-exact
/// with no gap or click at the wrap point; MP3 encoder delay/padding made
/// that unreliable when decoded through symphonia.
static BELL_WAV: &[u8] = include_bytes!("../assets/bell.wav");

const RELEASE_SECS: f32 = 0.18;
const RELEASE_FLOOR: f32 = 0.0008;

/// The bell clip fully decoded to raw samples. Looping over this in-memory
/// buffer is sample-exact and essentially free, unlike re-creating an MP3
/// decoder on every loop (which re-parses frames on the real-time audio
/// thread and causes an audible gap/underrun each time it restarts).
struct BellClip {
    samples: Vec<Sample>,
    channels: ChannelCount,
    sample_rate: SampleRate,
}

fn decode_clip() -> Result<BellClip, DecoderError> {
    let decoder = Decoder::new_wav(Cursor::new(BELL_WAV))?;
    let channels = decoder.channels();
    let sample_rate = decoder.sample_rate();
    let samples: Vec<Sample> = decoder.collect();
    Ok(BellClip {
        samples,
        channels,
        sample_rate,
    })
}

enum Stage {
    Sustain,
    Release,
}

/// Loops the bell clip for as long as the button/key is held, then fades it
/// out quickly on release instead of cutting it off with an audible click.
struct HeldBell {
    clip: Arc<BellClip>,
    pos: usize,
    released: Arc<AtomicBool>,
    stage: Stage,
    env: f32,
    release_coeff: f32,
}

impl HeldBell {
    fn new(clip: Arc<BellClip>, released: Arc<AtomicBool>) -> Self {
        let release_samples = (clip.sample_rate.get() as f32
            * RELEASE_SECS
            * clip.channels.get() as f32)
            .max(1.0);
        let release_coeff = RELEASE_FLOOR.powf(1.0 / release_samples);

        Self {
            clip,
            pos: 0,
            released,
            stage: Stage::Sustain,
            env: 1.0,
            release_coeff,
        }
    }
}

impl Iterator for HeldBell {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if self.clip.samples.is_empty() {
            return None;
        }

        match self.stage {
            Stage::Sustain => {
                if self.released.load(Ordering::Relaxed) {
                    self.stage = Stage::Release;
                }
            }
            Stage::Release => {
                self.env *= self.release_coeff;
                if self.env < RELEASE_FLOOR {
                    return None;
                }
            }
        }

        let sample = self.clip.samples[self.pos] * self.env;
        self.pos += 1;
        if self.pos >= self.clip.samples.len() {
            self.pos = 0;
        }
        Some(sample)
    }
}

impl Source for HeldBell {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        self.clip.channels
    }

    fn sample_rate(&self) -> SampleRate {
        self.clip.sample_rate
    }

    fn total_duration(&self) -> Option<Duration> {
        // Looping + held-duration-dependent, so there's no fixed length.
        None
    }
}

/// Spawns the dedicated audio thread that owns the output device and mixer,
/// and returns a channel to control it. Keeping all rodio/cpal objects on a
/// single thread sidesteps their platform-specific Send/Sync quirks.
pub fn spawn_audio_thread() -> Sender<AudioCommand> {
    let (tx, rx) = mpsc::channel::<AudioCommand>();

    thread::spawn(move || {
        let stream_handle = match DeviceSinkBuilder::open_default_sink() {
            Ok(v) => v,
            Err(err) => {
                eprintln!("bell: failed to open audio output: {err}");
                return;
            }
        };

        let clip = match decode_clip() {
            Ok(c) => Arc::new(c),
            Err(err) => {
                eprintln!("bell: failed to decode bell.mp3: {err}");
                return;
            }
        };

        // The source itself fades out and ends after `Stop`, so we only need
        // to keep its release flag around to know whether one is already
        // ringing (and to trigger the release envelope).
        let mut current: Option<Arc<AtomicBool>> = None;

        for cmd in rx {
            match cmd {
                AudioCommand::Start => {
                    if current.is_some() {
                        // Already ringing (e.g. OS key-repeat); ignore.
                        continue;
                    }
                    let released = Arc::new(AtomicBool::new(false));
                    let source = HeldBell::new(clip.clone(), released.clone());
                    stream_handle.mixer().add(source);
                    current = Some(released);
                }
                AudioCommand::Stop => {
                    if let Some(released) = current.take() {
                        released.store(true, Ordering::Relaxed);
                    }
                }
            }
        }
    });

    tx
}
