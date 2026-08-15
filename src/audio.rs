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
}

impl LowLatencySdlSink {
    fn open(format: AudioFormat) -> Self {
        let context = sdl2::init().expect("could not initialize SDL");
        let audio = context
            .audio()
            .expect("could not initialize SDL audio subsystem");
        let desired_spec = AudioSpecDesired {
            freq: Some(SAMPLE_RATE as i32),
            channels: Some(NUM_CHANNELS),
            samples: Some(512),
        };

        match format {
            AudioFormat::F32 => Self::F32(
                audio
                    .open_queue(None, &desired_spec)
                    .expect("could not open SDL audio device"),
            ),
            AudioFormat::S32 => Self::S32(
                audio
                    .open_queue(None, &desired_spec)
                    .expect("could not open SDL audio device"),
            ),
            AudioFormat::S16 => Self::S16(
                audio
                    .open_queue(None, &desired_spec)
                    .expect("could not open SDL audio device"),
            ),
            _ => panic!("SDL does not support {format:?} output"),
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
        }
        Ok(())
    }

    fn write(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
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
        }
    }
}

fn drain_queue<T: AudioFormatNum>(queue: &AudioQueue<T>, sample_size: usize) -> SinkResult<()> {
    let target_bytes = (SAMPLE_RATE as usize
        * NUM_CHANNELS as usize
        * sample_size
        * QUEUE_TARGET.as_millis() as usize
        / 1000) as u32;
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
