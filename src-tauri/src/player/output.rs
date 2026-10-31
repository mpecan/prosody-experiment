//! The phone's audio output, through `cpal` (`docs/player.md` §13): `AAudio` on Android, Core Audio on
//! Apple systems, WASAPI or ALSA on desktops. The device's own rate and channel count, 32-bit float
//! samples, its default buffer; the callback is the [`AudioSide`]'s.

use std::sync::Arc;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, Device, SampleFormat, Stream, StreamConfig};
use prosody_player::Event;

use super::host::{AudioSide, Shared};

/// The default output device and the configuration it will be opened with.
///
/// # Errors
///
/// No output, or none that takes 32-bit float samples at its own rate.
pub(crate) fn device() -> Result<(Device, StreamConfig), String> {
    let host = cpal::default_host();
    let device = host
        .default_output_device()
        .ok_or("this device has no audio output")?;
    let default = device
        .default_output_config()
        .map_err(|error| format!("the audio output has no configuration: {error}"))?;
    let rate = default.sample_rate();
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

/// Opens `device` with `config` and starts it, filling each buffer from `audio`. A stream error is
/// told to the page.
///
/// # Errors
///
/// The stream could not be opened or started.
pub(crate) fn start(
    device: &Device,
    config: StreamConfig,
    mut audio: AudioSide,
    shared: Arc<Shared>,
) -> Result<Stream, String> {
    let fill = Arc::clone(&shared);
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
                // queue's own underruns are (`CLAUDE.md` §6), and the music goes on.
                if error.kind() == cpal::ErrorKind::Xrun {
                    shared.status.glitched();
                } else {
                    log::error!("the audio output failed: {error}");
                    shared.tell(Event::Error(format!("the audio output failed: {error}")).json());
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
