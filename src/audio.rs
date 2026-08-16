use std::{thread, time::Duration};

use librespot::playback::{
    NUM_CHANNELS, SAMPLE_RATE,
    audio_backend::{Sink, SinkError, SinkResult},
    config::AudioFormat,
    convert::Converter,
    decoder::AudioPacket,
};
use sdl2::audio::{AudioFormatNum, AudioQueue, AudioSpecDesired, AudioStatus};

const QUEUE_TARGET: Duration = Duration::from_millis(150);
const DRAIN_TIMEOUT: Duration = Duration::from_secs(1);

pub fn low_latency_sdl_sink(_device: Option<String>, format: AudioFormat) -> Box<dyn Sink> {
    Box::new(LowLatencySdlSink::open(format))
}

enum LowLatencySdlSink {
    F32(AudioQueue<f32>),
    S32(AudioQueue<i32>),
    S16(AudioQueue<i16>),
    Failed(AudioOpenError),
}

enum AudioOpenError {
    Connection(String),
    InvalidFormat(String),
}

impl AudioOpenError {
    fn sink_error(&self) -> SinkError {
        match self {
            Self::Connection(error) => SinkError::ConnectionRefused(error.clone()),
            Self::InvalidFormat(error) => SinkError::InvalidParams(error.clone()),
        }
    }
}

impl LowLatencySdlSink {
    fn open(format: AudioFormat) -> Self {
        if !matches!(
            format,
            AudioFormat::F32 | AudioFormat::S32 | AudioFormat::S16
        ) {
            return Self::Failed(AudioOpenError::InvalidFormat(format!(
                "SDL does not support {format:?} output"
            )));
        }
        let context = match sdl2::init() {
            Ok(context) => context,
            Err(error) => {
                return Self::Failed(AudioOpenError::Connection(format!(
                    "could not initialize SDL: {error}"
                )));
            }
        };
        let audio = match context.audio() {
            Ok(audio) => audio,
            Err(error) => {
                return Self::Failed(AudioOpenError::Connection(format!(
                    "could not initialize SDL audio subsystem: {error}"
                )));
            }
        };
        let Ok(sample_rate) = i32::try_from(SAMPLE_RATE) else {
            return Self::Failed(AudioOpenError::InvalidFormat(
                "audio sample rate exceeds SDL range".to_owned(),
            ));
        };
        let desired_spec = AudioSpecDesired {
            freq: Some(sample_rate),
            channels: Some(NUM_CHANNELS),
            samples: Some(512),
        };

        match format {
            AudioFormat::F32 => audio.open_queue(None, &desired_spec).map_or_else(
                |error| Self::Failed(AudioOpenError::Connection(error)),
                Self::F32,
            ),
            AudioFormat::S32 => audio.open_queue(None, &desired_spec).map_or_else(
                |error| Self::Failed(AudioOpenError::Connection(error)),
                Self::S32,
            ),
            AudioFormat::S16 => audio.open_queue(None, &desired_spec).map_or_else(
                |error| Self::Failed(AudioOpenError::Connection(error)),
                Self::S16,
            ),
            _ => Self::Failed(AudioOpenError::InvalidFormat(format!(
                "SDL does not support {format:?} output"
            ))),
        }
    }
}

impl Sink for LowLatencySdlSink {
    fn start(&mut self) -> SinkResult<()> {
        match self {
            Self::F32(queue) => {
                queue.clear();
                queue.resume();
            }
            Self::S32(queue) => {
                queue.clear();
                queue.resume();
            }
            Self::S16(queue) => {
                queue.clear();
                queue.resume();
            }
            Self::Failed(error) => return Err(error.sink_error()),
        }
        Ok(())
    }

    fn stop(&mut self) -> SinkResult<()> {
        match self {
            Self::F32(queue) => {
                queue.pause();
                queue.clear();
            }
            Self::S32(queue) => {
                queue.pause();
                queue.clear();
            }
            Self::S16(queue) => {
                queue.pause();
                queue.clear();
            }
            Self::Failed(_) => {}
        }
        Ok(())
    }

    fn write(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
        if let Self::Failed(error) = self {
            return Err(error.sink_error());
        }
        let samples = packet
            .samples()
            .map_err(|error| SinkError::OnWrite(error.to_string()))?;
        match self {
            Self::F32(queue) => {
                drain_queue(queue, std::mem::size_of::<f32>())?;
                queue
                    .queue_audio(&converter.f64_to_f32(samples))
                    .map_err(SinkError::OnWrite)
            }
            Self::S32(queue) => {
                drain_queue(queue, std::mem::size_of::<i32>())?;
                queue
                    .queue_audio(&converter.f64_to_s32(samples))
                    .map_err(SinkError::OnWrite)
            }
            Self::S16(queue) => {
                drain_queue(queue, std::mem::size_of::<i16>())?;
                queue
                    .queue_audio(&converter.f64_to_s16(samples))
                    .map_err(SinkError::OnWrite)
            }
            Self::Failed(error) => Err(error.sink_error()),
        }
    }
}

fn drain_queue<T: AudioFormatNum>(queue: &AudioQueue<T>, sample_size: usize) -> SinkResult<()> {
    let target_bytes = u128::from(SAMPLE_RATE)
        * u128::from(NUM_CHANNELS)
        * sample_size as u128
        * QUEUE_TARGET.as_millis()
        / 1000;
    let target_bytes = u32::try_from(target_bytes).unwrap_or(u32::MAX);
    let deadline = std::time::Instant::now() + DRAIN_TIMEOUT;
    while queue.size() > target_bytes {
        if queue.status() != AudioStatus::Playing {
            return Err(SinkError::StateChange(
                "SDL audio device stopped consuming samples".to_owned(),
            ));
        }
        if std::time::Instant::now() >= deadline {
            queue.clear();
            queue.resume();
            return Ok(());
        }
        thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_audio_formats_return_sink_errors() {
        let mut sink = LowLatencySdlSink::open(AudioFormat::F64);

        assert!(matches!(sink.start(), Err(SinkError::InvalidParams(_))));
    }
}
