use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum DownloadError {
    #[error("yt-dlp not found. Install it: https://github.com/yt-dlp/yt-dlp")]
    YtDlpNotFound,
    #[error("ffmpeg not found. Install it: https://ffmpeg.org")]
    FfmpegNotFound,
    #[error("yt-dlp failed: {0}")]
    YtDlpFailed(String),
    #[error("could not extract video ID from: {0}")]
    InvalidUrl(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Extract the video ID from a YouTube URL or bare ID.
pub fn extract_video_id(url: &str) -> Result<String, DownloadError> {
    // Already a bare ID (no slashes, no dots)
    if !url.contains('/') && !url.contains('.') {
        return Ok(url.to_string());
    }

    // Try to find v= parameter
    if let Some(pos) = url.find("v=") {
        let id = &url[pos + 2..]; // ignore v=
        let id = id.split(['&', '#']).next().unwrap_or(id); // Eventual queries or frags.
        if !id.is_empty() {
            return Ok(id.to_string());
        }
    }

    // youtu.be/<id> short links
    if let Some(pos) = url.find("youtu.be/") {
        let id = &url[pos + 9..];
        let id = id.split(['?', '&', '#']).next().unwrap_or(id);
        if !id.is_empty() {
            return Ok(id.to_string());
        }
    }

    Err(DownloadError::InvalidUrl(url.to_string()))
}

/// Build a full YouTube URL from a URL or bare video ID.
fn to_full_url(url: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("https://www.youtube.com/watch?v={url}")
    }
}

fn check_dependency(name: &str) -> Result<(), DownloadError> {
    let result = Command::new("which").arg(name).output();
    match result {
        Ok(output) if output.status.success() => Ok(()),
        _ => match name {
            "yt-dlp" => Err(DownloadError::YtDlpNotFound),
            "ffmpeg" => Err(DownloadError::FfmpegNotFound),
            _ => unreachable!(),
        },
    }
}

fn secs_to_hms(secs: f64) -> String {
    let total = secs as u64;
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

/// Download audio from a YouTube URL or video ID as WAV.
///
/// `clip` — optional `(start_secs, end_secs)` to download only that range.
/// Returns the path to the downloaded file.
pub fn download(url: &str, output_dir: &str, clip: Option<(f64, f64)>) -> Result<PathBuf, DownloadError> {
    check_dependency("yt-dlp")?;
    check_dependency("ffmpeg")?;

    let video_id = extract_video_id(url)?;
    let full_url = to_full_url(url);
    let out_path = Path::new(output_dir);

    fs::create_dir_all(out_path)?;

    let stem = match clip {
        Some((start, end)) => format!("{video_id}_{start}_{end}"),
        None => video_id.clone(),
    };

    let output_template = out_path.join(format!("{stem}.%(ext)s"));
    let wav_path = out_path.join(format!("{stem}.wav"));

    // yt-dlp: download and convert to wav via ffmpeg postprocessor,
    // forcing 16kHz mono (required by whisper.cpp)
    let mut cmd = Command::new("yt-dlp");
    cmd.args([
        "--extract-audio",
        "--audio-format",
        "wav",
        "--postprocessor-args",
        "ffmpeg:-ar 16000 -ac 1",
    ]);

    if let Some((start, end)) = clip {
        let section = format!("*{}-{}", secs_to_hms(start), secs_to_hms(end));
        cmd.args(["--download-sections", &section]);
    }

    cmd.arg("--output")
        .arg(output_template.to_str().unwrap_or_default())
        .arg(&full_url);

    let output = cmd.output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DownloadError::YtDlpFailed(stderr.into_owned()));
    }

    if wav_path.exists() {
        Ok(wav_path)
    } else {
        Err(DownloadError::YtDlpFailed(
            "download succeeded but WAV file not found".into(),
        ))
    }
}
