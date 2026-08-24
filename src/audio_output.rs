use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use cpal::{
    FromSample, SampleFormat, SizedSample, Stream, StreamConfig, SupportedStreamConfig,
    SupportedStreamConfigRange,
    traits::{DeviceTrait as _, HostTrait as _, StreamTrait as _},
};
use librespot::playback::{
    NUM_CHANNELS, SAMPLE_RATE,
    audio_backend::{Sink, SinkError, SinkResult},
    config::AudioFormat,
    convert::Converter,
    decoder::AudioPacket,
};
use ringbuf::{
    HeapCons, HeapProd, HeapRb,
    traits::{Consumer as _, Producer as _, Split as _},
};
use rubato::{Fft, FixedSync, Resampler as _, audioadapter_buffers::direct::InterleavedSlice};

const QUEUE_TARGET: Duration = Duration::from_millis(150);
const MAX_QUEUE_BYTES: usize = 64 * 1024 * 1024;
const RESAMPLER_CHUNK_FRAMES: usize = 256;
const WRITE_TIMEOUT: Duration = Duration::from_secs(1);
const RECOVERY_TIMEOUT: Duration = Duration::from_secs(5);
const RECOVERY_RETRY_DELAY: Duration = Duration::from_millis(100);
const OUTPUT_FORMATS: [SampleFormat; 6] = [
    SampleFormat::F32,
    SampleFormat::I16,
    SampleFormat::I32,
    SampleFormat::U16,
    SampleFormat::U32,
    SampleFormat::F64,
];

fn wait_until_cleared(
    clear_requested: &AtomicBool,
    failed: &AtomicBool,
    timeout: Duration,
) -> SinkResult<()> {
    let deadline = Instant::now() + timeout;
    while clear_requested.load(Ordering::Acquire) {
        if failed.load(Ordering::Acquire) {
            return Err(SinkError::StateChange(
                "audio output stream failed".to_owned(),
            ));
        }
        if Instant::now() >= deadline {
            return Err(SinkError::StateChange(
                "audio output did not start consuming samples".to_owned(),
            ));
        }
        thread::sleep(Duration::from_millis(2));
    }
    if failed.load(Ordering::Acquire) {
        Err(SinkError::StateChange(
            "audio output stream failed".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn recover_failed_write(
    result: SinkResult<()>,
    stream_failed: bool,
    recover: impl FnOnce() -> SinkResult<()>,
) -> SinkResult<()> {
    if result.is_err() && stream_failed {
        recover()
    } else {
        result
    }
}

fn retry_recovery(mut attempt: impl FnMut(Instant) -> (SinkResult<()>, bool)) -> SinkResult<()> {
    let deadline = Instant::now() + RECOVERY_TIMEOUT;
    loop {
        let (result, retryable) = attempt(deadline);
        match result {
            Ok(()) => return Ok(()),
            Err(error) if retryable => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(error);
                }
                thread::sleep(RECOVERY_RETRY_DELAY.min(remaining));
                if Instant::now() >= deadline {
                    return Err(error);
                }
            }
            Err(error) => return Err(error),
        }
    }
}

fn select_supported_config(
    configs: &[SupportedStreamConfigRange],
    channels: Option<u16>,
    mut resolve: impl FnMut(SupportedStreamConfigRange) -> Option<SupportedStreamConfig>,
) -> Option<SupportedStreamConfig> {
    OUTPUT_FORMATS.into_iter().find_map(|format| {
        configs
            .iter()
            .filter(|config| {
                config.sample_format() == format
                    && channels.is_none_or(|channels| config.channels() == channels)
            })
            .find_map(|config| resolve(*config))
    })
}

fn select_nearest_config(configs: &[SupportedStreamConfigRange]) -> Option<SupportedStreamConfig> {
    OUTPUT_FORMATS.into_iter().find_map(|format| {
        configs
            .iter()
            .filter(|config| config.sample_format() == format)
            .map(|config| {
                let sample_rate =
                    SAMPLE_RATE.clamp(config.min_sample_rate(), config.max_sample_rate());
                (sample_rate.abs_diff(SAMPLE_RATE), *config, sample_rate)
            })
            .min_by_key(|(distance, _, _)| *distance)
            .and_then(|(_, config, sample_rate)| config.try_with_sample_rate(sample_rate))
    })
}

fn select_output_config(
    configs: &[SupportedStreamConfigRange],
    default: Option<SupportedStreamConfig>,
) -> Option<SupportedStreamConfig> {
    let stereo = Some(u16::from(NUM_CHANNELS));
    select_supported_config(configs, stereo, |config| {
        config.try_with_sample_rate(SAMPLE_RATE)
    })
    .or_else(|| {
        select_supported_config(configs, stereo, |config| {
            config.try_with_standard_sample_rate()
        })
    })
    .or_else(|| {
        default.filter(|config| {
            config.channels() > 0 && OUTPUT_FORMATS.contains(&config.sample_format())
        })
    })
    .or_else(|| {
        select_supported_config(configs, None, |config| {
            config.try_with_sample_rate(SAMPLE_RATE)
        })
    })
    .or_else(|| {
        select_supported_config(configs, None, |config| {
            config.try_with_standard_sample_rate()
        })
    })
    .or_else(|| select_nearest_config(configs))
}

fn queue_capacity(sample_rate: u32, channels: u16) -> Result<usize, String> {
    let samples = u128::from(sample_rate) * QUEUE_TARGET.as_millis() / 1_000 * u128::from(channels);
    let samples = usize::try_from(samples)
        .map_err(|_| "audio output queue size exceeds platform range".to_owned())?;
    if samples > MAX_QUEUE_BYTES / std::mem::size_of::<f32>() {
        return Err("audio output queue would exceed 64 MiB".to_owned());
    }
    Ok(samples)
}

fn map_output_channels(samples: Vec<f32>, output_channels: u16) -> Vec<f32> {
    match output_channels {
        0 => Vec::new(),
        2 => samples,
        1 => samples
            .chunks_exact(2)
            .map(|frame| (frame[0] + frame[1]) * 0.5)
            .collect(),
        channels => {
            let channels = usize::from(channels);
            let mut mapped = Vec::with_capacity(samples.len() / 2 * channels);
            for frame in samples.chunks_exact(2) {
                mapped.extend_from_slice(frame);
                mapped.resize(mapped.len() + channels - 2, 0.0);
            }
            mapped
        }
    }
}

fn playback_buffer(capacity: usize) -> (BufferWriter, BufferReader) {
    let (producer, consumer) = HeapRb::new(capacity).split();
    (BufferWriter(producer), BufferReader(consumer))
}

struct BufferWriter(HeapProd<f32>);

impl BufferWriter {
    fn write(&mut self, samples: &[f32]) -> usize {
        self.0.push_slice(samples)
    }

    fn write_all(
        &mut self,
        samples: &[f32],
        playing: &AtomicBool,
        failed: &AtomicBool,
    ) -> SinkResult<()> {
        let deadline = Instant::now() + WRITE_TIMEOUT;
        let mut written = 0;
        while written < samples.len() {
            if failed.load(Ordering::Acquire) {
                return Err(SinkError::OnWrite("audio output stream failed".to_owned()));
            }
            if !playing.load(Ordering::Acquire) {
                return Err(SinkError::StateChange(
                    "audio output is not playing".to_owned(),
                ));
            }
            written += self.write(&samples[written..]);
            if written < samples.len() {
                if Instant::now() >= deadline {
                    return Err(SinkError::OnWrite(
                        "audio output stopped consuming samples".to_owned(),
                    ));
                }
                thread::sleep(Duration::from_millis(2));
            }
        }
        Ok(())
    }
}

struct BufferReader(HeapCons<f32>);

impl BufferReader {
    fn clear(&mut self) {
        self.0.clear();
    }

    fn fill_converted<T>(&mut self, output: &mut [T])
    where
        T: FromSample<f32> + SizedSample,
    {
        for sample in output {
            *sample = T::from_sample(self.0.try_pop().unwrap_or(0.0));
        }
    }
}

enum OutputConverter {
    Passthrough,
    Resampling {
        resampler: Box<Fft<f32>>,
        pending: Vec<f32>,
    },
}

impl OutputConverter {
    fn new(input_rate: u32, output_rate: u32) -> Result<Self, String> {
        if input_rate == output_rate {
            return Ok(Self::Passthrough);
        }
        let input_rate = usize::try_from(input_rate)
            .map_err(|_| "input sample rate exceeds platform range".to_owned())?;
        let output_rate = usize::try_from(output_rate)
            .map_err(|_| "output sample rate exceeds platform range".to_owned())?;
        let resampler = Fft::new(
            input_rate,
            output_rate,
            RESAMPLER_CHUNK_FRAMES,
            usize::from(NUM_CHANNELS),
            FixedSync::Input,
        )
        .map_err(|error| format!("could not configure audio resampling: {error}"))?;
        Ok(Self::Resampling {
            resampler: Box::new(resampler),
            pending: Vec::new(),
        })
    }

    fn convert(&mut self, samples: Vec<f32>) -> Result<Vec<f32>, String> {
        let Self::Resampling { resampler, pending } = self else {
            return Ok(samples);
        };

        pending.extend_from_slice(&samples);
        let frames_per_chunk = resampler.input_frames_next();
        let channels = usize::from(NUM_CHANNELS);
        let samples_per_chunk = frames_per_chunk
            .checked_mul(channels)
            .ok_or_else(|| "audio resampling chunk size overflowed".to_owned())?;
        let complete_chunks = pending.len() / samples_per_chunk;
        let mut converted = Vec::new();
        for chunk in pending[..complete_chunks * samples_per_chunk].chunks_exact(samples_per_chunk)
        {
            let input = InterleavedSlice::new(chunk, channels, frames_per_chunk)
                .map_err(|error| format!("could not prepare audio for resampling: {error}"))?;
            let output = resampler
                .process(&input, None)
                .map_err(|error| format!("could not resample audio: {error}"))?;
            converted.extend(output.take_data());
        }
        pending.drain(..complete_chunks * samples_per_chunk);
        Ok(converted)
    }

    fn reset(&mut self) {
        if let Self::Resampling {
            resampler, pending, ..
        } = self
        {
            pending.clear();
            resampler.reset();
        }
    }
}

pub(crate) fn open(device: Option<String>, format: AudioFormat) -> Box<dyn Sink> {
    Box::new(CpalSink::open(device, format))
}

enum CpalSink {
    Ready(Box<Playback>),
    Failed(AudioOpenError),
    Reopening,
}

struct Playback {
    stream: Stream,
    writer: BufferWriter,
    converter: OutputConverter,
    output_channels: u16,
    clear_requested: Arc<AtomicBool>,
    playing: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
}

enum AudioOpenError {
    Connection(String),
    InvalidInput(String),
    InvalidFormat(String),
}

impl AudioOpenError {
    fn sink_error(&self) -> SinkError {
        match self {
            Self::Connection(error) => SinkError::ConnectionRefused(error.clone()),
            Self::InvalidInput(error) | Self::InvalidFormat(error) => {
                SinkError::InvalidParams(error.clone())
            }
        }
    }

    fn retryable(&self) -> bool {
        !matches!(self, Self::InvalidInput(_))
    }
}

impl CpalSink {
    fn open(device_name: Option<String>, format: AudioFormat) -> Self {
        if format != AudioFormat::F32 {
            return Self::Failed(AudioOpenError::InvalidInput(format!(
                "audio output does not support {format:?} samples"
            )));
        }
        if device_name.is_some() {
            return Self::Failed(AudioOpenError::InvalidInput(
                "selecting a named audio output is not supported".to_owned(),
            ));
        }

        Self::open_playback()
    }

    fn open_playback() -> Self {
        match Playback::open() {
            Ok(playback) => Self::Ready(Box::new(playback)),
            Err(error) => Self::Failed(error),
        }
    }

    fn playback_mut(&mut self) -> SinkResult<&mut Playback> {
        match self {
            Self::Ready(playback) => Ok(playback),
            Self::Failed(error) => Err(error.sink_error()),
            Self::Reopening => unreachable!(),
        }
    }

    fn stream_failed(&self) -> bool {
        matches!(self, Self::Ready(playback) if playback.failed.load(Ordering::Acquire))
    }

    fn recovery_retryable(&self) -> bool {
        match self {
            Self::Ready(playback) => playback.failed.load(Ordering::Acquire),
            Self::Failed(error) => error.retryable(),
            Self::Reopening => false,
        }
    }

    fn recover_output(&mut self) -> SinkResult<()> {
        retry_recovery(|deadline| {
            let result = self.start_before(deadline);
            (result, self.recovery_retryable())
        })
    }

    fn recover_stream_failure(&mut self) -> SinkResult<()> {
        log::warn!("audio output stream failed; reopening the default output");
        self.recover_output()
    }

    fn start_before(&mut self, deadline: Instant) -> SinkResult<()> {
        let should_reopen = match self {
            Self::Ready(playback) => playback.failed.load(Ordering::Acquire),
            Self::Failed(error) => error.retryable(),
            Self::Reopening => unreachable!(),
        };
        if should_reopen {
            let previous = std::mem::replace(self, Self::Reopening);
            drop(previous);
            *self = Self::open_playback();
        }

        let playback = self.playback_mut()?;
        playback.clear_requested.store(true, Ordering::Release);
        playback.converter.reset();
        if let Err(error) = playback.stream.play() {
            playback.failed.store(true, Ordering::Release);
            return Err(SinkError::StateChange(error.to_string()));
        }
        let timeout = deadline.saturating_duration_since(Instant::now());
        if let Err(error) = wait_until_cleared(&playback.clear_requested, &playback.failed, timeout)
        {
            playback.failed.store(true, Ordering::Release);
            let _ = playback.stream.pause();
            return Err(error);
        }
        playback.playing.store(true, Ordering::Release);
        Ok(())
    }

    fn write_packet(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
        let playback = self.playback_mut()?;
        let samples = packet
            .samples()
            .map_err(|error| SinkError::OnWrite(error.to_string()))?;
        let samples = converter.f64_to_f32(samples);
        let samples = playback
            .converter
            .convert(samples)
            .map_err(SinkError::OnWrite)?;
        let samples = map_output_channels(samples, playback.output_channels);
        playback
            .writer
            .write_all(&samples, &playback.playing, &playback.failed)
    }
}

impl Playback {
    fn open() -> Result<Self, AudioOpenError> {
        let device = cpal::default_host()
            .default_output_device()
            .ok_or_else(|| {
                AudioOpenError::Connection("no default audio output is available".to_owned())
            })?;
        let supported_configs = device
            .supported_output_configs()
            .map_err(|error| {
                AudioOpenError::Connection(format!("could not query audio output formats: {error}"))
            })?
            .filter(|config| {
                config.channels() > 0 && OUTPUT_FORMATS.contains(&config.sample_format())
            })
            .collect::<Vec<_>>();
        let output_config =
            select_output_config(&supported_configs, device.default_output_config().ok())
                .ok_or_else(|| {
                    AudioOpenError::InvalidFormat(
                        "default audio output does not support a compatible format".to_owned(),
                    )
                })?;
        let sample_format = output_config.sample_format();
        let config = output_config.config();
        let capacity = queue_capacity(config.sample_rate, config.channels)
            .map_err(AudioOpenError::InvalidFormat)?;
        let converter = OutputConverter::new(SAMPLE_RATE, config.sample_rate)
            .map_err(AudioOpenError::InvalidFormat)?;
        let (writer, reader) = playback_buffer(capacity);
        let clear_requested = Arc::new(AtomicBool::new(true));
        let playing = Arc::new(AtomicBool::new(false));
        let failed = Arc::new(AtomicBool::new(false));
        let stream = match sample_format {
            SampleFormat::F32 => build_output_stream::<f32>(
                &device,
                config,
                reader,
                Arc::clone(&clear_requested),
                Arc::clone(&failed),
            ),
            SampleFormat::I16 => build_output_stream::<i16>(
                &device,
                config,
                reader,
                Arc::clone(&clear_requested),
                Arc::clone(&failed),
            ),
            SampleFormat::U16 => build_output_stream::<u16>(
                &device,
                config,
                reader,
                Arc::clone(&clear_requested),
                Arc::clone(&failed),
            ),
            SampleFormat::I32 => build_output_stream::<i32>(
                &device,
                config,
                reader,
                Arc::clone(&clear_requested),
                Arc::clone(&failed),
            ),
            SampleFormat::U32 => build_output_stream::<u32>(
                &device,
                config,
                reader,
                Arc::clone(&clear_requested),
                Arc::clone(&failed),
            ),
            SampleFormat::F64 => build_output_stream::<f64>(
                &device,
                config,
                reader,
                Arc::clone(&clear_requested),
                Arc::clone(&failed),
            ),
            _ => unreachable!("sample format was filtered above"),
        }?;

        Ok(Self {
            stream,
            writer,
            converter,
            output_channels: config.channels,
            clear_requested,
            playing,
            failed,
        })
    }
}

fn build_output_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut reader: BufferReader,
    clear_requested: Arc<AtomicBool>,
    failed: Arc<AtomicBool>,
) -> Result<Stream, AudioOpenError>
where
    T: FromSample<f32> + SizedSample,
{
    device
        .build_output_stream(
            config,
            move |output: &mut [T], _| {
                if clear_requested.load(Ordering::Acquire) {
                    reader.clear();
                    clear_requested.store(false, Ordering::Release);
                }
                reader.fill_converted(output);
            },
            move |_| {
                failed.store(true, Ordering::Release);
            },
            None,
        )
        .map_err(|error| {
            AudioOpenError::Connection(format!("could not open audio output: {error}"))
        })
}

impl Sink for CpalSink {
    fn start(&mut self) -> SinkResult<()> {
        self.recover_output()
    }

    fn stop(&mut self) -> SinkResult<()> {
        if let Self::Ready(playback) = self {
            playback.playing.store(false, Ordering::Release);
            playback.clear_requested.store(true, Ordering::Release);
            if let Err(error) = playback.stream.pause() {
                playback.failed.store(true, Ordering::Release);
                log::warn!("could not pause audio output: {error}");
            }
        }
        Ok(())
    }

    fn write(&mut self, packet: AudioPacket, converter: &mut Converter) -> SinkResult<()> {
        if self.stream_failed() {
            self.recover_stream_failure()?;
        }

        let result = self.write_packet(packet, converter);
        let stream_failed = self.stream_failed();
        recover_failed_write(result, stream_failed, || self.recover_stream_failure())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::Duration,
    };

    use cpal::{
        SampleFormat, SupportedBufferSize, SupportedStreamConfig, SupportedStreamConfigRange,
    };
    use librespot::playback::{audio_backend::SinkError, config::AudioFormat};

    use super::{
        AudioOpenError, OutputConverter, map_output_channels, open, playback_buffer,
        queue_capacity, recover_failed_write, retry_recovery, select_output_config,
        wait_until_cleared,
    };

    #[test]
    fn unsupported_audio_formats_return_sink_errors() {
        let mut sink = open(None, AudioFormat::F64);

        assert!(matches!(sink.start(), Err(SinkError::InvalidParams(_))));
    }

    #[test]
    fn integer_input_formats_are_rejected_instead_of_ignoring_dithering() {
        for format in [AudioFormat::S16, AudioFormat::S32] {
            let mut sink = open(None, format);

            assert!(matches!(sink.start(), Err(SinkError::InvalidParams(_))));
        }
    }

    #[test]
    fn hardware_open_failures_are_retryable_but_invalid_input_is_not() {
        assert!(AudioOpenError::Connection(String::new()).retryable());
        assert!(AudioOpenError::InvalidFormat(String::new()).retryable());
        assert!(!AudioOpenError::InvalidInput(String::new()).retryable());
    }

    #[test]
    fn playback_buffer_fills_underruns_with_silence() {
        let (mut writer, mut reader) = playback_buffer(4);
        writer.write(&[0.25, -0.5]);
        let mut output = [1.0; 4];

        reader.fill_converted(&mut output);

        assert_eq!(output, [0.25, -0.5, 0.0, 0.0]);
    }

    #[test]
    fn clearing_playback_buffer_discards_queued_audio() {
        let (mut writer, mut reader) = playback_buffer(4);
        writer.write(&[0.25, -0.5]);

        reader.clear();
        let mut output = [1.0; 2];
        reader.fill_converted(&mut output);

        assert_eq!(output, [0.0, 0.0]);
    }

    #[test]
    fn playback_buffer_holds_150_milliseconds() {
        assert_eq!(queue_capacity(44_100, 2).unwrap(), 13_230);
    }

    #[test]
    fn playback_buffer_rejects_unbounded_device_formats() {
        assert!(queue_capacity(u32::MAX, u16::MAX).is_err());
    }

    #[test]
    fn start_waits_for_the_callback_to_clear_stale_audio() {
        let clear_requested = Arc::new(AtomicBool::new(true));
        let failed = AtomicBool::new(false);
        let callback_clear = Arc::clone(&clear_requested);
        let callback = thread::spawn(move || {
            thread::sleep(Duration::from_millis(1));
            callback_clear.store(false, Ordering::Release);
        });

        wait_until_cleared(&clear_requested, &failed, Duration::from_secs(1)).unwrap();
        callback.join().unwrap();
    }

    #[test]
    fn startup_fails_if_the_stream_failed_while_clearing() {
        let clear_requested = AtomicBool::new(false);
        let failed = AtomicBool::new(true);

        assert!(matches!(
            wait_until_cleared(&clear_requested, &failed, Duration::from_secs(1)),
            Err(SinkError::StateChange(_))
        ));
    }

    #[test]
    fn queued_writes_fail_immediately_after_stream_failure() {
        let (mut writer, _reader) = playback_buffer(1);
        writer.write(&[0.25]);
        let playing = AtomicBool::new(true);
        let failed = AtomicBool::new(true);

        let result = writer.write_all(&[0.5], &playing, &failed);

        assert!(matches!(result, Err(SinkError::OnWrite(_))));
    }

    #[test]
    fn recovered_stream_failures_do_not_escape_to_librespot() {
        let write_result = Err(SinkError::OnWrite("stream failed".to_owned()));
        let mut recovered = false;

        let result = recover_failed_write(write_result, true, || {
            recovered = true;
            Ok(())
        });

        assert!(result.is_ok());
        assert!(recovered);
    }

    #[test]
    fn transient_device_handoffs_are_retried() {
        let mut attempts = 0;

        let result = retry_recovery(|_| {
            attempts += 1;
            if attempts == 1 {
                (
                    Err(SinkError::ConnectionRefused(
                        "device unavailable".to_owned(),
                    )),
                    true,
                )
            } else {
                (Ok(()), false)
            }
        });

        assert!(result.is_ok());
        assert_eq!(attempts, 2);
    }

    #[test]
    fn sample_rate_conversion_has_bounded_non_accumulating_delay() {
        let mut converter = OutputConverter::new(44_100, 48_000).unwrap();
        let input = vec![0.25; 44_100 * 2];

        let first_output = converter.convert(input.clone()).unwrap();
        let second_output = converter.convert(input).unwrap();

        assert_eq!(first_output.len() / 2, 47_680);
        assert_eq!(second_output.len() / 2, 48_000);
    }

    #[test]
    fn stereo_audio_is_mapped_to_the_device_channel_count() {
        let input = vec![0.5, -0.25, 1.0, -0.5];

        assert_eq!(map_output_channels(input.clone(), 1), vec![0.125, 0.25]);
        assert_eq!(
            map_output_channels(input, 4),
            vec![0.5, -0.25, 0.0, 0.0, 1.0, -0.5, 0.0, 0.0]
        );
    }

    #[test]
    fn output_config_falls_back_to_a_nonstandard_sample_rate() {
        let configs = [
            SupportedStreamConfigRange::new(
                2,
                192_000,
                192_000,
                SupportedBufferSize::Unknown,
                SampleFormat::F32,
            ),
            SupportedStreamConfigRange::new(
                2,
                40_000,
                40_000,
                SupportedBufferSize::Unknown,
                SampleFormat::F32,
            ),
        ];

        let selected = select_output_config(&configs, None).unwrap();

        assert_eq!(selected.sample_rate(), 40_000);
    }

    #[test]
    fn output_config_prefers_stereo_over_a_multichannel_default() {
        let configs = [SupportedStreamConfigRange::new(
            2,
            48_000,
            48_000,
            SupportedBufferSize::Unknown,
            SampleFormat::F32,
        )];
        let default =
            SupportedStreamConfig::new(6, 44_100, SupportedBufferSize::Unknown, SampleFormat::F32);

        let selected = select_output_config(&configs, Some(default)).unwrap();

        assert_eq!(selected.channels(), 2);
        assert_eq!(selected.sample_rate(), 48_000);
    }
}
