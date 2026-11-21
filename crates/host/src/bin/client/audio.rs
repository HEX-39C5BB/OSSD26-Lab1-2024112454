use std::collections::VecDeque;
use std::io;
use std::num::NonZeroU32;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use cpal::{
    BufferSize, SupportedBufferSize,
    traits::{DeviceTrait, HostTrait},
};
use rodio::{
    DeviceSinkBuilder, DeviceSinkError, MixerDeviceSink, Player, Source, buffer::SamplesBuffer,
    mixer::Mixer, source::SineWave,
};
use rustcraft_authority::SoundKind;
use rustcraft_scene::assets::{AudioClip, NATIVE_BUNDLE, Pack};

use super::settings::AudioSettings;

type Error = Box<dyn std::error::Error>;
const MAX_VOICES: usize = 12;
const PREFERRED_BUFFER_FRAMES: u32 = 1024;

pub(super) struct SoundBank {
    place: SamplesBuffer,
    break_block: SamplesBuffer,
    step: SamplesBuffer,
}

impl SoundBank {
    pub fn new(pack: &Pack<'_>) -> io::Result<Self> {
        let native = Pack::open(NATIVE_BUNDLE)?;
        let clip = |name| {
            pack.audio(name)
                .or_else(|| native.audio(name))
                .map(mono_source)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, format!("missing audio {name}"))
                })
        };
        Ok(Self {
            place: clip("rustcraft_content:block_place")?,
            break_block: clip("rustcraft_content:block_break")?,
            step: clip("rustcraft_content:footstep")?,
        })
    }

    fn source(&self, kind: SoundKind) -> SamplesBuffer {
        match kind {
            SoundKind::Place => self.place.clone(),
            SoundKind::Break => self.break_block.clone(),
            SoundKind::Step => self.step.clone(),
        }
    }
}

fn mono_source(clip: AudioClip<'_>) -> SamplesBuffer {
    let decode = |bytes: [u8; 2]| f32::from(i16::from_le_bytes(bytes)) / 32768.0;
    let samples: Vec<_> = if clip.channels == 1 {
        clip.pcm
            .as_chunks::<2>()
            .0
            .iter()
            .copied()
            .map(decode)
            .collect()
    } else {
        clip.pcm
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&[a, b, c, d]| (decode([a, b]) + decode([c, d])) * 0.5)
            .collect()
    };
    SamplesBuffer::new(
        rodio::nz!(1),
        NonZeroU32::new(clip.sample_rate).expect("validated sample rate"),
        samples,
    )
}

pub(super) struct SoundEffects {
    mixer: Mixer,
    bank: SoundBank,
    voices: VecDeque<Player>,
    gain: f32,
}

impl SoundEffects {
    pub fn new(mixer: Mixer, bank: SoundBank, settings: AudioSettings) -> Self {
        Self {
            mixer,
            bank,
            voices: VecDeque::with_capacity(MAX_VOICES),
            gain: settings.gain(),
        }
    }

    pub fn settings_changed(&mut self, settings: AudioSettings) {
        self.gain = settings.gain();
        if self.gain == 0.0 {
            self.stop();
        } else {
            for voice in &self.voices {
                voice.set_volume(self.gain);
            }
        }
    }

    pub fn play(&mut self, kind: SoundKind) {
        if self.gain == 0.0 {
            return;
        }
        self.voices.retain(|voice| !voice.empty());
        if self.voices.len() == MAX_VOICES {
            // Dropping a Rodio Player stops its queued source.
            self.voices.pop_front();
        }
        let voice = Player::connect_new(&self.mixer);
        voice.set_volume(self.gain);
        voice.append(self.bank.source(kind));
        self.voices.push_back(voice);
    }

    pub fn stop(&mut self) {
        self.voices.clear();
    }

    fn idle(&self) -> bool {
        self.voices.iter().all(Player::empty)
    }
}

pub(super) fn open() -> Result<MixerDeviceSink, Error> {
    let device = open_default(|error| eprintln!("audio stream error: {error}"))
        .or_else(|error| DeviceSinkBuilder::open_default_sink().map_err(|_| error))?;
    Ok(ready(device))
}

fn open_default(
    on_error: impl FnMut(cpal::Error) + Clone + Send + 'static,
) -> Result<MixerDeviceSink, DeviceSinkError> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or(DeviceSinkError::NoDevice)?;
    let supported = device
        .default_output_config()
        .map_err(DeviceSinkError::DefaultSinkConfigError)?;
    let buffer_size = match *supported.buffer_size() {
        SupportedBufferSize::Range { min, max } if max >= min.max(1) => {
            PREFERRED_BUFFER_FRAMES.clamp(min.max(1), max)
        }
        _ => PREFERRED_BUFFER_FRAMES,
    };
    let builder = || {
        DeviceSinkBuilder::default()
            .with_device(device.clone())
            .with_supported_config(&supported)
            .with_error_callback(on_error.clone())
    };
    let sink = match builder()
        .with_buffer_size(BufferSize::Fixed(buffer_size))
        .open_stream()
    {
        Err(DeviceSinkError::BuildError(error))
            if error.kind() == cpal::ErrorKind::UnsupportedConfig =>
        {
            builder()
                .with_buffer_size(BufferSize::Default)
                .open_stream()
        }
        result => result,
    }?;
    let description = device.description().ok();
    eprintln!(
        "event=audio_output_selected backend={} id={:?} name={:?}",
        host.id().name(),
        device.id().ok().map(|id| id.to_string()),
        description.as_ref().map(|description| description.name())
    );
    Ok(sink)
}

fn ready(mut device: MixerDeviceSink) -> MixerDeviceSink {
    device.log_on_drop(false);
    let config = device.config();
    eprintln!(
        "event=audio_device_ready channels={} sample_rate={} buffer_request={:?}",
        config.channel_count(),
        config.sample_rate(),
        config.buffer_size()
    );
    device
}

pub(super) fn smoke(bank: SoundBank) -> Result<(), Error> {
    let (send_error, stream_errors) = mpsc::sync_channel(1);
    let mut send_error = Some(send_error);
    let device = ready(open_default(move |error| {
        if let Some(sender) = send_error.take() {
            let _ = sender.try_send(error);
        }
    })?);

    let check_stream = |stage: &str, wait| -> Result<(), Error> {
        match stream_errors.recv_timeout(wait) {
            Ok(error) => Err(format!("audio output failed during {stage}: {error}").into()),
            Err(RecvTimeoutError::Timeout) => Ok(()),
            Err(RecvTimeoutError::Disconnected) => Err("audio stream stopped unexpectedly".into()),
        }
    };
    let wait_for_mix = |stage: &str, idle: &dyn Fn() -> bool| -> Result<(), Error> {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !idle() {
            if Instant::now() >= deadline {
                return Err(format!("audio mixer did not finish {stage}").into());
            }
            check_stream(stage, Duration::from_millis(10))?;
        }
        Ok(())
    };

    let reference_duration = Duration::from_secs(2);
    let settle = Duration::from_millis(300);
    let reference = Player::connect_new(device.mixer());
    eprintln!("event=audio_smoke_reference frequency_hz=440 duration_ms=2000");
    let started = Instant::now();
    reference.append(
        SineWave::new(440.0)
            .take_duration(reference_duration)
            .fade_in(Duration::from_millis(20))
            .amplify(0.30),
    );
    wait_for_mix("reference", &|| reference.empty())?;
    check_stream(
        "reference",
        reference_duration.saturating_sub(started.elapsed()) + settle,
    )?;

    let settings = AudioSettings {
        volume_percent: 100,
        muted: false,
    };
    let mut sounds = SoundEffects::new(device.mixer().clone(), bank, settings);
    for cue in [SoundKind::Place, SoundKind::Break, SoundKind::Step] {
        eprintln!("event=audio_smoke_cue cue={cue:?}");
        sounds.play(cue);
        let stage = format!("{cue:?}");
        wait_for_mix(&stage, &|| sounds.idle())?;
        check_stream(&stage, settle)?;
    }
    eprintln!("event=audio_smoke_passed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (SoundEffects, rodio::mixer::MixerSource) {
        let bank = SoundBank::new(&Pack::open(NATIVE_BUNDLE).unwrap()).unwrap();
        let (mixer, source) = rodio::mixer::mixer(rodio::nz!(2), rodio::nz!(24000));
        (
            SoundEffects::new(mixer, bank, AudioSettings::default()),
            source,
        )
    }

    #[test]
    fn mixes_native_clips_in_stereo_center_and_honors_master_gain() {
        fn render(kind: SoundKind, volume_percent: u8) -> Vec<f32> {
            let (mut sounds, source) = fixture();
            sounds.settings_changed(AudioSettings {
                volume_percent,
                muted: false,
            });
            sounds.play(kind);
            source.take(2000).collect()
        }

        for kind in [SoundKind::Place, SoundKind::Break, SoundKind::Step] {
            let full = render(kind, 100);
            let half = render(kind, 50);
            assert!(full.iter().any(|sample| sample.abs() > 0.02));
            assert!(
                full.as_chunks::<2>()
                    .0
                    .iter()
                    .all(|pair| pair[0] == pair[1])
            );
            assert!(
                full.iter()
                    .zip(&half)
                    .all(|(&a, &b)| (a * 0.5 - b).abs() < 1e-6)
            );
        }
    }

    #[test]
    fn pause_or_mute_does_not_replay_queued_effects_after_resume() {
        let (mut sounds, mut source) = fixture();
        // An audio callback pulls a fixed number of samples and fills idle slots with zero.
        let mut pull = |count| {
            (0..count)
                .map(|_| source.next().unwrap_or(0.0))
                .collect::<Vec<_>>()
        };
        sounds.play(SoundKind::Break);
        assert!(pull(600).iter().any(|sample| sample.abs() > 0.02));
        sounds.stop();
        // Rodio checks a playing player's stop signal at a sub-frame interval.
        assert!(pull(1260)[300..].iter().all(|sample| sample.abs() < 1e-7));

        sounds.settings_changed(AudioSettings {
            volume_percent: 70,
            muted: true,
        });
        sounds.play(SoundKind::Place);
        assert!(pull(800).iter().all(|&sample| sample == 0.0));
        sounds.settings_changed(AudioSettings::default());
        assert!(pull(800).iter().all(|&sample| sample == 0.0));
        sounds.play(SoundKind::Step);
        assert!(pull(900).iter().any(|sample| sample.abs() > 0.02));
    }
}
