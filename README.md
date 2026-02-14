# Sawtrs

[![CI](https://github.com/amasotti/sawtrs/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/amasotti/sawtrs/actions/workflows/ci.yml)
[![Built with Rust](https://img.shields.io/badge/Built_with-Rust-dea584?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Whisper](https://img.shields.io/badge/Whisper-whisper.cpp-74aa9c?logo=openai&logoColor=white)](https://github.com/ggml-org/whisper.cpp)
[![Ollama](https://img.shields.io/badge/Embeddings-Ollama-000000?logo=ollama&logoColor=white)](https://ollama.com)

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

```
sawt download <url> [-o dir]              # Download audio from YouTube
sawt transcribe <file> [--language XX]    # Transcribe a WAV file, display table
sawt search "query" [-n N] [--video-id]   # Semantic search over stored transcripts
sawt export <video-id> [-o file.csv]      # Export stored transcript as table + CSV
sawt pipeline <url> [--language XX]       # Full: download → transcribe → store
```

Global flag: `-v` enables debug logging.

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
