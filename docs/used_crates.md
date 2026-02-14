# Crates Used in Sawtrs

## clap (v4, derive feature)

`clap` is the de facto standard for command-line argument parsing in Rust. With the `derive` feature enabled, you define
your CLI structure as Rust structs and enums, and clap generates the parsing logic, help text, and validation at compile time. 
It handles subcommands, global flags, default values, and optional arguments out of the box. 
The derive approach is preferred over the builder API for most projects  because it keeps the CLI definition declarative 
and co-located with the data types. In our case, `Cli` holds the global `--verbose` flag, and `Command` is an enum with 
one variant per subcommand (`Download`, `Transcribe`, `Search`,`Export`, `Pipeline`).

## anyhow (v1)

`anyhow` provides a flexible, opaque error type (`anyhow::Error`) for application-level code where you don't need
callers to match on specific error variants. It wraps any error that implements `std::error::Error` and supports context
chaining via `.context("what happened")`. It's meant for binaries and top-level orchestration — not for library APIs
where callers need to programmatically distinguish error cases. In sawtrs, `anyhow` is used (or will be used) in
`main.rs` to propagate errors from modules without re-wrapping them into a single unified type. The convention is:
`anyhow` for the binary, `thiserror` for the libraries.

## thiserror (v2)

`thiserror` is a derive macro for implementing `std::error::Error` on custom error enums with minimal boilerplate. You
annotate each variant with `#[error("...")]` to define its `Display` message, and use `#[from]` to auto-generate `From`
impls for transparent error conversion. Unlike `anyhow`, `thiserror` produces typed, matchable errors — making it the
right choice for library modules where callers need to handle specific failure modes. In sawtrs, each module (
`downloader`, `transcriber`, `store`, `export`) defines its own error enum via `thiserror`, e.g.
`DownloadError::YtDlpNotFound`, `TranscribeError::ModelNotFound`, or `StoreError::OllamaUnavailable`. This keeps error
handling structured without writing manual `impl` blocks.

## whisper-rs (v0.15)

`whisper-rs` provides Rust bindings to [whisper.cpp](https://github.com/ggml-org/whisper.cpp), OpenAI's Whisper speech
recognition model compiled as a C++ library. It compiles whisper.cpp from source via `cc`/`cmake` during `cargo build`,
so no separate C++ build step is needed. The API flow is: create a `WhisperContext` from a GGML model file, spawn a
`WhisperState`, configure `FullParams` (sampling strategy, language, beam size), then call
`state.full(params, &samples)` which runs the full pipeline (PCM → log-mel spectrogram → encoder → decoder → text).
Segments are extracted via `state.as_iter()`. 

**Important**: whisper.cpp expects **16kHz mono f32 audio** — it does no
resampling internally, unlike Python's `faster-whisper`. 

The `SamplingStrategy::BeamSearch { beam_size: 5 }` variant matches the Python implementation's default. 
Optional features include `cuda`, `metal`, and `coreml` for GPU acceleration, though CPU is the default and works well 
on macOS with Apple Silicon.

## hound (v3)

`hound` is a pure-Rust library for reading and writing WAV audio files. It parses the WAV header to extract sample
format (int or float), bit depth, sample rate, and channel count, then provides an iterator over the raw samples. It
supports both `i16`/`i32` integer and `f32` float sample formats. In sawtrs, I use it to load WAV files and convert the
samples to `f32` for `whisper.cpp`. I also handle stereo-to-mono downmixing manually (averaging the two channels).
`hound` does not do any resampling — it reads the samples as stored in the file, which is why the download step must
produce 16kHz WAV.

## comfy-table (v7)

`comfy-table` renders formatted ASCII/Unicode tables in the terminal. You create a `Table`, set headers, add rows, and
call `println!("{table}")` — it handles column widths, alignment, and border styles automatically. It auto-detects
terminal width (via the `tty` feature) and wraps content to fit. In sawtrs, it's used to display transcription
segments (index, start time, end time, text) and is reused for search results and export previews. It's a
lightweight alternative to `tabled` with a simpler API.

## usearch (v2)

`usearch` is a compact, single-file vector search engine implementing the HNSW ([Hierarchical Navigable Small World](https://en.wikipedia.org/wiki/Hierarchical_navigable_small_world))
algorithm. It stores `(u64 key, vector)` pairs and supports approximate nearest-neighbor search with cosine, L2, and
inner-product metrics. The Rust API is a thin wrapper over C++ via `cxx`. Key operations: `Index::new(&options)` to
create, `add(key, &vector)` to insert, `search(&query, n)` to find neighbors (returns `Matches { keys, distances }`),
`remove(key)` to delete, and `save`/`load` for persistence. It also supports `filtered_search` with a predicate
closure — we use this for the `--video-id` filter in search. usearch stores only vectors and keys, not metadata, so we
pair it with a sidecar `metadata.json` file that maps each `u64` key to its segment info (video ID, timestamps, text).
