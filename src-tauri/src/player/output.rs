//! The phone's audio output, through `cpal` (`docs/player.md` §13): `AAudio` on Android, Core Audio on
//! Apple systems, WASAPI or ALSA on desktops. The device's own rate and channel count, 32-bit float
//! samples, its default buffer; the callback is the [`AudioSide`]'s.
//!
//! The render thread holds the [`Output`]: it reopens it when it fails, since an `AAudio` stream that
//! loses its device (headphones in or out, the audio server restarting) never calls back again and
//! must not be reopened from its own callback; and it pauses it while nothing plays, so an idle app
//! does not keep the phone's audio path powered.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, Device, SampleFormat, Stream, StreamConfig};
use prosody_player::Mixer;

use super::host::{AudioSide, Shared, ToAudio};
use super::rounds::Sink;

/// The default output device and the configuration it will be opened with: at `rate` when one is
/// given (a reopened output keeps the rate the engine renders at and the page decoded its samples
/// at), at the device's own otherwise.
///
/// # Errors
///
/// No output, or none that takes 32-bit float samples at that rate.
pub(crate) fn device(rate: Option<u32>) -> Result<(Device, StreamConfig), String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("this device has no audio output")?;
    let default = device
        .default_output_config()
        .map_err(|error| format!("the audio output has no configuration: {error}"))?;
    let rate = rate.unwrap_or_else(|| default.sample_rate());
    let channels = default.channels();
    let floats = device
        .supported_output_configs()
        .map_err(|error| format!("the audio output's configurations: {error}"))?
        .any(|range| {
            range.sample_format() == SampleFormat::F32
                && range.channels() == channels
                && range.min_sample_rate() <= rate
                && rate <= range.max_sample_rate()
        });
    if !floats {
        return Err(format!(
            "the audio output takes no 32-bit float samples at {rate} Hz with {channels} channels"
        ));
    }
    Ok((
        device,
        StreamConfig {
            channels,
            sample_rate: rate,
            buffer_size: BufferSize::Default,
        },
    ))
}

/// An open output: its stream, the rate it was opened at, and where its callback sends finished
/// blocks back.
pub(crate) struct Output {
    stream: Option<Stream>,
    rate: u32,
    back: SyncSender<Vec<f32>>,
}

impl Output {
    /// Opens `device` with `config` and starts it, its callback taking blocks from `from` and
    /// sending them back down `back`.
    ///
    /// # Errors
    ///
    /// The stream could not be opened or started.
    pub(crate) fn start(
        device: &Device,
        config: StreamConfig,
        from: Receiver<ToAudio>,
        back: SyncSender<Vec<f32>>,
        shared: &Arc<Shared>,
    ) -> Result<Self, String> {
        let rate = config.sample_rate;
        let stream = stream(device, config, from, back.clone(), shared)?;
        Ok(Self {
            stream: Some(stream),
            rate,
            back,
        })
    }
}

impl Sink for Output {
    fn reopen(
        &mut self,
        shared: &Arc<Shared>,
        outbox: &mut VecDeque<ToAudio>,
    ) -> Result<(), String> {
        // The dead stream first: its callbacks end with it, so none of them is heard from again.
        drop(self.stream.take());
        let (device, config) = device(Some(self.rate))?;
        let from = shared.reconnect(outbox);
        self.stream = Some(stream(&device, config, from, self.back.clone(), shared)?);
        Ok(())
    }

    fn pause(&mut self) -> Result<(), String> {
        self.stream.as_ref().map_or(Ok(()), |stream| {
            stream
                .pause()
                .map_err(|error| format!("the audio output would not pause: {error}"))
        })
    }

    fn resume(&mut self) -> Result<(), String> {
        self.stream.as_ref().map_or(Ok(()), |stream| {
            stream
                .play()
                .map_err(|error| format!("the audio output would not start again: {error}"))
        })
    }
}

/// Opens `device` with `config` and starts it, filling each buffer from a fresh mixer fed by
/// `from`. An xrun is counted; any other error marks the output lost, for the render thread to
/// reopen.
fn stream(
    device: &Device,
    config: StreamConfig,
    from: Receiver<ToAudio>,
    back: SyncSender<Vec<f32>>,
    shared: &Arc<Shared>,
) -> Result<Stream, String> {
    let mut audio = AudioSide {
        mixer: Mixer::new(f64::from(config.sample_rate)),
        from,
        back,
        channels: usize::from(config.channels),
    };
    let fill = Arc::clone(shared);
    let failed = Arc::clone(shared);
    let stream = device
        .build_output_stream(
            config,
            move |out: &mut [f32], info: &cpal::OutputCallbackInfo| {
                let at = info.timestamp();
                let latency = at
                    .playback
                    .checked_duration_since(at.callback)
                    .map_or(0, |ahead| u64::try_from(ahead.as_nanos()).unwrap_or(0));
                audio.fill(out, &fill, latency);
            },
            move |error: cpal::Error| {
                // An xrun is a glitch, not a failure: counted and said with the losses, as the
                // queue's own underruns are (`CLAUDE.md` §6), and the music goes on. Anything else
                // — on Android, a disconnected device, mapped by cpal to `DeviceNotAvailable` — ends
                // the stream's callbacks for good: marked for the render thread to reopen it.
                if error.kind() == cpal::ErrorKind::Xrun {
                    failed.status.glitched();
                } else {
                    log::error!("the audio output failed: {error}");
                    failed.lose();
                }
            },
            None,
        )
        .map_err(|error| format!("the audio output would not open: {error}"))?;
    stream
        .play()
        .map_err(|error| format!("the audio output would not start: {error}"))?;
    Ok(stream)
}
