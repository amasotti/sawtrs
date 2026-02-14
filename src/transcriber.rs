use std::path::Path;

use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

#[derive(Debug, Clone)]
pub struct Segment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TranscribeError {
    #[error("WAV file not found: {0}")]
    FileNotFound(String),
    #[error("model file not found: {0}")]
    ModelNotFound(String),
    #[error("whisper error: {0}")]
    Whisper(#[from] whisper_rs::WhisperError),
    #[error("failed to read WAV: {0}")]
    Wav(String),
}

const DEFAULT_MODEL_DIR: &str = "models";
const DEFAULT_MODEL_NAME: &str = "whisper-large-v3-turbo.bin"; // ggml model for whisper.cpp
//const DEFAULT_MODEL_NAME: &str = "ggml-large-v3.bin"; // ggml model for whisper.cpp

/// Resolve the model path: use provided path or fall back to `models/ggml-large-v3.bin`.
fn resolve_model_path(model_path: Option<&str>) -> Result<String, TranscribeError> {
    if let Some(p) = model_path {
        if Path::new(p).exists() {
            return Ok(p.to_string());
        }
        return Err(TranscribeError::ModelNotFound(p.to_string()));
    }

    let default = format!("{DEFAULT_MODEL_DIR}/{DEFAULT_MODEL_NAME}");
    if Path::new(&default).exists() {
        return Ok(default);
    }

    Err(TranscribeError::ModelNotFound(format!(
        "{default} (download a ggml model from https://github.com/ggml-org/whisper.cpp)"
    )))
}

/// Read a WAV file and return mono f32 samples at 16kHz.
fn read_wav(path: &str) -> Result<Vec<f32>, TranscribeError> {
    let reader = hound::WavReader::open(path)
        .map_err(|e| TranscribeError::Wav(format!("{path}: {e}")))?;

    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .filter_map(Result::ok)
            .collect(),
        hound::SampleFormat::Int => {
            let max_val = (1u32 << (spec.bits_per_sample - 1)) as f32;
            reader
                .into_samples::<i32>()
                .filter_map(Result::ok)
                .map(|s| s as f32 / max_val)
                .collect()
        }
    };

    // Convert to mono if stereo
    let mono = if spec.channels == 2 {
        samples
            .chunks(2)
            .map(|chunk| (chunk[0] + chunk.get(1).copied().unwrap_or(0.0)) / 2.0)
            .collect()
    } else {
        samples
    };

    Ok(mono)
}

/// Transcribe a WAV file using Whisper.
///
/// - `file`: path to the WAV file (must be 16kHz or will be interpreted as-is by whisper.cpp)
/// - `language`: optional language code (e.g. "en", "it", "ar"). `None` for auto-detection.
/// - `model_path`: optional path to a ggml model file. Defaults to `models/ggml-large-v3.bin`.
pub fn transcribe(
    file: &str,
    language: Option<&str>,
    model_path: Option<&str>,
) -> Result<Vec<Segment>, TranscribeError> {
    if !Path::new(file).exists() {
        return Err(TranscribeError::FileNotFound(file.to_string()));
    }

    let model = resolve_model_path(model_path)?;
    let ctx = WhisperContext::new_with_params(&model, WhisperContextParameters::default())?;
    let mut state = ctx.create_state()?;

    let mut params = FullParams::new(SamplingStrategy::BeamSearch {
        beam_size: 5,
        patience: -1.0,
    });
    params.set_language(language);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);

    let samples = read_wav(file)?;
    state.full(params, &samples)?;

    let segments: Vec<Segment> = state
        .as_iter()
        .map(|seg| Segment {
            start: seg.start_timestamp() as f64 / 100.0,
            end: seg.end_timestamp() as f64 / 100.0,
            text: seg
                .to_str_lossy()
                .unwrap_or_default()
                .trim()
                .to_string(),
        })
        .collect();

    Ok(segments)
}

/// Format segments as a console table.
pub fn format_table(segments: &[Segment]) -> comfy_table::Table {
    let mut table = comfy_table::Table::new();
    table.set_header(["#", "Start", "End", "Text"]);

    for (i, seg) in segments.iter().enumerate() {
        table.add_row([
            (i + 1).to_string(),
            format_timestamp(seg.start),
            format_timestamp(seg.end),
            seg.text.clone(),
        ]);
    }

    table
}

fn format_timestamp(seconds: f64) -> String {
    let mins = (seconds / 60.0) as u32;
    let secs = seconds % 60.0;
    format!("{mins:02}:{secs:05.2}")
}
