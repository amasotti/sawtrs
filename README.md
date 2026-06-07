# Sawtrs

[![CI](https://github.com/amasotti/sawtrs/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/amasotti/sawtrs/actions/workflows/ci.yml)
[![Built with Rust](https://img.shields.io/badge/Built_with-Rust-dea584?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Whisper](https://img.shields.io/badge/Whisper-whisper.cpp-74aa9c?logo=openai&logoColor=white)](https://github.com/ggml-org/whisper.cpp)
[![Ollama](https://img.shields.io/badge/Embeddings-Ollama-000000?logo=ollama&logoColor=white)](https://ollama.com)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

S2T (sawt as in arabic صوت, "voice") is a local CLI tool that downloads YouTube audio, transcribes it, stores transcript segments in a vector database,
and allows semantic search and export.

This is a Rust port of [sawtpy](https://github.com/amasotti/sawtpy), originally written in Python.
Both are small learning projects for me, to interact with embeddings and transcription models without relying on external APIs.

## Prerequisites (local tools)

- [yt-dlp](https://github.com/yt-dlp/yt-dlp) — must be installed and available on `PATH`
- [FFmpeg](https://ffmpeg.org) — must be installed and available on `PATH`
- A [whisper.cpp ggml model](https://huggingface.co/ggerganov/whisper.cpp/tree/main) — place it in `models/` (defaults to `models/whisper-large-v3-turbo.bin`)
- [Ollama](https://ollama.com) — must be running (`ollama serve`), with the embedding model pulled: `ollama pull nomic-embed-text`

## Pipeline

```
YouTube URL/ID → download audio (WAV) → transcribe (Whisper) → store segments → search/export
```

## CLI Interface

Global flag: `-v` / `--verbose` — enable debug logging.

### `sawtrs download`

Download audio from YouTube as a 16 kHz mono WAV.

```
sawtrs download <URL> [OPTIONS]

Arguments:
  <URL>              YouTube URL or bare video ID

Options:
  -o, --output <DIR>   Output directory [default: downloads]
      --start <SECS>   Clip start time in seconds (requires --end)
      --end <SECS>     Clip end time in seconds (requires --start)
```

Examples:
```bash
sawtrs download https://youtube.com/watch?v=ABC123
sawtrs download ABC123 -o /tmp/audio
sawtrs download https://youtube.com/watch?v=ABC123 --start 90 --end 240
```

Clips save as `{video_id}_{start}_{end}.wav` so multiple clips from the same video don't collide.
`--start` and `--end` must be provided together.

### `sawtrs transcribe`

Transcribe a WAV file with Whisper and print a segment table.

```
sawtrs transcribe <FILE> [OPTIONS]

Arguments:
  <FILE>             Path to WAV file

Options:
      --language <LANG>   Language code, e.g. en, it, ar (omit for auto-detection)
```

Examples:
```bash
sawtrs transcribe downloads/ABC123.wav
sawtrs transcribe downloads/ABC123.wav --language en
```

### `sawtrs search`

Semantic search over all stored transcript segments.

```
sawtrs search <QUERY> [OPTIONS]

Arguments:
  <QUERY>            Search query (natural language)

Options:
  -n <N>              Number of results [default: 5]
      --video-id <ID>  Restrict search to a single video
```

Examples:
```bash
sawtrs search "climate change policy"
sawtrs search "climate change" -n 10
sawtrs search "climate change" --video-id ABC123
```

### `sawtrs export`

Print all stored segments for a video and optionally write a CSV.

```
sawtrs export <VIDEO_ID> [OPTIONS]

Arguments:
  <VIDEO_ID>         Video ID to export

Options:
  -o, --output <FILE>   Write CSV to this path (columns: start, end, text)
```

Examples:
```bash
sawtrs export ABC123
sawtrs export ABC123 -o transcript.csv
```

### `sawtrs pipeline`

Full pipeline: download → transcribe → store in one step.

```
sawtrs pipeline <URL> [OPTIONS]

Arguments:
  <URL>              YouTube URL or bare video ID

Options:
      --language <LANG>   Language code for transcription (omit for auto-detection)
```

Examples:
```bash
sawtrs pipeline https://youtube.com/watch?v=ABC123
sawtrs pipeline ABC123 --language ar
```

## Modules

There are four independent modules. 
The CLI binary is the composition root — modules never import each other.

### Downloader

- Input: YouTube URL or bare video ID (auto-prefixed to full URL).
- Calls yt-dlp + FFmpeg to extract audio as WAV.
- Saves to `downloads/<video_id>.wav`.
- Returns the file path or an error.

### Transcriber

- Input: path to a WAV file, optional language code (e.g. `en`, `it`, `ar`).
- Runs Whisper (large-v3 model) with beam search (size 5).
- Auto-detects device: prefers CPU/int8 on macOS, CUDA/float16 if available.
- `None` language triggers auto-detection.
- Returns a list of segments: `{ start: f64, end: f64, text: String }`.

### Vector Store

- Stores transcript segments with embeddings for semantic search.
- Uses `nomic-embed-text` embeddings (768 dimensions) via Ollama (needs to be available locally).
- Vector index stored with usearch (HNSW), metadata in a sidecar JSON file.
- Segment IDs are deterministic (`{video_id}_{index}` → FNV-1a hash) so re-ingestion is idempotent (upsert).
- Operations: `store_transcript`, `search` (with optional video_id filter), `get_segments` (all segments for a video
  sorted by start time), `get_video_ids`, `delete_video`.

### Export

- Retrieves all stored segments for a video ID.
- Prints a formatted table to the console.
- Writes a CSV file with columns: `start, end, text`.
- Exits with error if the video has no stored transcript.
