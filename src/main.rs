mod downloader;
mod transcriber;

use clap::{Parser, Subcommand};

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
            println!(
                "search: not implemented (query={query}, n={n}, video_id={})",
                video_id.as_deref().unwrap_or("all")
            );
        }
        Command::Export { video_id, output } => {
            println!(
                "export: not implemented (video_id={video_id}, output={})",
                output.as_deref().unwrap_or("stdout")
            );
        }
        Command::Pipeline { url, language } => {
            println!(
                "pipeline: not implemented (url={url}, language={})",
                language.as_deref().unwrap_or("auto")
            );
        }
    }
}
