mod downloader;
#[allow(dead_code)]
mod export;
#[allow(dead_code)]
mod store;
mod transcriber;

use clap::{Parser, Subcommand};

const STORE_DIR: &str = "store_data";

#[derive(Parser)]
#[command(name = "sawt", about = "Download, transcribe, search and export YouTube audio")]
struct Cli {
    /// debug logging
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Download audio from YouTube
    Download {
        /// YouTube URL or video ID
        url: String,
        /// Output directory
        #[arg(short, long, default_value = "downloads")]
        output: String,
    },

    /// Transcribe a WAV file
    Transcribe {
        /// Path to WAV file
        file: String,
        /// Language code (e.g. en, it, ar). Omit for auto-detection
        #[arg(long)]
        language: Option<String>,
    },

    /// Semantic search over stored transcripts
    Search {
        /// Search query
        query: String,
        /// Number of results
        #[arg(short, long, default_value = "5")]
        n: usize,
        /// Filter by video ID
        #[arg(long)]
        video_id: Option<String>,
    },

    /// Export stored transcript as table + CSV
    Export {
        /// Video ID to export
        video_id: String,
        /// Output CSV file path
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Full pipeline: download → transcribe → store
    Pipeline {
        /// YouTube URL or video ID
        url: String,
        /// Language code (e.g. en, it, ar). Omit for auto-detection
        #[arg(long)]
        language: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    if cli.verbose {
        eprintln!("[verbose mode enabled]");
    }

    match cli.command {
        Command::Download { url, output } => {
            match downloader::download(&url, &output) {
                Ok(path) => println!("downloaded: {}", path.display()),
                Err(e) => eprintln!("error: {e}"),
            }
        }
        Command::Transcribe { file, language } => {
            match transcriber::transcribe(&file, language.as_deref(), None) {
                Ok(segments) => {
                    println!("{}", transcriber::format_table(&segments));
                    println!("{} segment(s)", segments.len());
                }
                Err(e) => eprintln!("error: {e}"),
            }
        }
        Command::Search { query, n, video_id } => {
            match store::VectorStore::open(STORE_DIR) {
                Ok(vs) => match vs.search(&query, n, video_id.as_deref()) {
                    Ok(results) if results.is_empty() => {
                        println!("no results found");
                    }
                    Ok(results) => {
                        let mut table = comfy_table::Table::new();
                        table.set_header(["#", "Video", "Time", "Text", "Distance"]);
                        for (i, r) in results.iter().enumerate() {
                            table.add_row([
                                (i + 1).to_string(),
                                r.video_id.clone(),
                                format!(
                                    "{}-{}",
                                    format_ts(r.start),
                                    format_ts(r.end)
                                ),
                                r.text.clone(),
                                format!("{:.4}", r.distance),
                            ]);
                        }
                        println!("{table}");
                        println!("{} result(s)", results.len());
                    }
                    Err(e) => eprintln!("error: {e}"),
                },
                Err(e) => eprintln!("error: {e}"),
            }
        }
        Command::Export { video_id, output } => {
            match store::VectorStore::open(STORE_DIR) {
                Ok(vs) => match vs.get_segments(&video_id) {
                    Ok(segments) => {
                        let export_segs: Vec<export::ExportSegment> = segments
                            .iter()
                            .map(|s| export::ExportSegment {
                                index: s.index,
                                start: s.start,
                                end: s.end,
                                text: s.text.clone(),
                            })
                            .collect();

                        println!("{}", export::format_table(&video_id, &export_segs));
                        println!("{} segment(s)", export_segs.len());

                        if let Some(path) = output {
                            match export::write_csv(&path, &export_segs) {
                                Ok(()) => println!("written to {path}"),
                                Err(e) => eprintln!("csv error: {e}"),
                            }
                        }
                    }
                    Err(e) => eprintln!("error: {e}"),
                },
                Err(e) => eprintln!("error: {e}"),
            }
        }
        Command::Pipeline { url, language } => {
            // Step 1: Download
            eprintln!("[1/3] downloading audio...");
            let wav_path = match downloader::download(&url, "downloads") {
                Ok(path) => {
                    eprintln!("       saved to {}", path.display());
                    path
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    return;
                }
            };

            // Step 2: Transcribe
            eprintln!("[2/3] transcribing...");
            let segments = match transcriber::transcribe(
                wav_path.to_str().unwrap_or_default(),
                language.as_deref(),
                None,
            ) {
                Ok(segs) => {
                    eprintln!("       {} segment(s)", segs.len());
                    segs
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    return;
                }
            };

            // Step 3: Store
            eprintln!("[3/3] storing in vector database...");
            let video_id = match downloader::extract_video_id(&url) {
                Ok(id) => id,
                Err(e) => {
                    eprintln!("error: {e}");
                    return;
                }
            };

            let store_segments: Vec<store::TranscriptSegment> = segments
                .iter()
                .map(|s| store::TranscriptSegment {
                    start: s.start,
                    end: s.end,
                    text: s.text.clone(),
                })
                .collect();

            match store::VectorStore::open(STORE_DIR) {
                Ok(mut vs) => match vs.store_transcript(&video_id, &store_segments) {
                    Ok(n) => eprintln!("       stored {n} segment(s) for {video_id}"),
                    Err(e) => eprintln!("error: {e}"),
                },
                Err(e) => eprintln!("error: {e}"),
            }
        }
    }
}

fn format_ts(seconds: f64) -> String {
    let mins = (seconds / 60.0) as u32;
    let secs = seconds % 60.0;
    format!("{mins:02}:{secs:05.2}")
}
