use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

const EMBEDDING_DIM: usize = 768;
const OLLAMA_EMBED_URL: &str = "http://localhost:11434/api/embed";
const EMBEDDING_MODEL: &str = "nomic-embed-text";
const INDEX_FILE: &str = "index.usearch";
const METADATA_FILE: &str = "metadata.json";

// ── Error type ──────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Ollama is not running at {OLLAMA_EMBED_URL}. Start it with: ollama serve")]
    OllamaUnavailable,
    #[error("embedding failed: {0}")]
    EmbeddingFailed(String),
    #[error("index error: {0}")]
    Index(String),
    #[error("video not found: {0}")]
    VideoNotFound(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
}

// ── Public types ────────────────────────────────────────────────────────

/// Input segment — mirrors transcriber::Segment but defined independently.
#[derive(Debug, Clone)]
pub struct TranscriptSegment {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

/// Segment stored in metadata.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredSegment {
    pub video_id: String,
    pub index: usize,
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub key: u64,
}

/// Result returned by search.
#[derive(Debug, Clone)]
pub struct SearchResult {
    pub video_id: String,
    pub start: f64,
    pub end: f64,
    pub text: String,
    pub distance: f32,
}

// ── Deterministic ID: FNV-1a ────────────────────────────────────────────

fn fnv1a_hash(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in s.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

// ── Ollama embeddings ───────────────────────────────────────────────────

#[derive(Serialize)]
struct EmbedRequest<'a> {
    model: &'a str,
    input: Vec<&'a str>,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

fn embed_texts(texts: &[&str]) -> Result<Vec<Vec<f32>>, StoreError> {
    let client = reqwest::blocking::Client::new();
    let body = EmbedRequest {
        model: EMBEDDING_MODEL,
        input: texts.to_vec(),
    };

    let resp = client
        .post(OLLAMA_EMBED_URL)
        .json(&body)
        .send()
        .map_err(|e| {
            if e.is_connect() {
                StoreError::OllamaUnavailable
            } else {
                StoreError::Http(e)
            }
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        return Err(StoreError::EmbeddingFailed(format!(
            "HTTP {status}: {body}"
        )));
    }

    let parsed: EmbedResponse = resp.json()?;
    if parsed.embeddings.len() != texts.len() {
        return Err(StoreError::EmbeddingFailed(format!(
            "expected {} embeddings, got {}",
            texts.len(),
            parsed.embeddings.len()
        )));
    }

    Ok(parsed.embeddings)
}

// ── VectorStore ─────────────────────────────────────────────────────────

pub struct VectorStore {
    data_dir: PathBuf,
    index: Index,
    metadata: HashMap<u64, StoredSegment>,
}

impl VectorStore {
    /// Create or load a vector store from `data_dir`.
    pub fn open(data_dir: &str) -> Result<Self, StoreError> {
        let data_dir = PathBuf::from(data_dir);
        fs::create_dir_all(&data_dir)?;

        let options = IndexOptions {
            dimensions: EMBEDDING_DIM,
            metric: MetricKind::Cos,
            quantization: ScalarKind::F32,
            connectivity: 16,
            expansion_add: 128,
            expansion_search: 64,
            multi: false,
        };

        let index = Index::new(&options).map_err(|e| StoreError::Index(e.to_string()))?;

        let index_path = data_dir.join(INDEX_FILE);
        if index_path.exists() {
            index
                .load(index_path.to_str().unwrap_or_default())
                .map_err(|e| StoreError::Index(e.to_string()))?;
        }

        let metadata_path = data_dir.join(METADATA_FILE);
        let metadata: HashMap<u64, StoredSegment> = if metadata_path.exists() {
            let data = fs::read_to_string(&metadata_path)?;
            serde_json::from_str(&data)?
        } else {
            HashMap::new()
        };

        Ok(Self {
            data_dir,
            index,
            metadata,
        })
    }

    /// Embed and store transcript segments for a video.
    pub fn store_transcript(
        &mut self,
        video_id: &str,
        segments: &[TranscriptSegment],
    ) -> Result<usize, StoreError> {
        if segments.is_empty() {
            return Ok(0);
        }

        let texts: Vec<&str> = segments.iter().map(|s| s.text.as_str()).collect();
        let embeddings = embed_texts(&texts)?;

        // Reserve capacity for new entries
        let new_capacity = self.index.size() + segments.len();
        self.index
            .reserve(new_capacity)
            .map_err(|e| StoreError::Index(e.to_string()))?;

        for (i, (seg, embedding)) in segments.iter().zip(embeddings.iter()).enumerate() {
            let key_str = format!("{video_id}_{i}");
            let key = fnv1a_hash(&key_str);

            // Remove old entry if it exists (idempotent upsert)
            if self.index.contains(key) {
                let _ = self.index.remove(key);
            }

            self.index
                .add(key, embedding)
                .map_err(|e| StoreError::Index(e.to_string()))?;

            self.metadata.insert(
                key,
                StoredSegment {
                    video_id: video_id.to_string(),
                    index: i,
                    start: seg.start,
                    end: seg.end,
                    text: seg.text.clone(),
                    key,
                },
            );
        }

        self.persist()?;
        Ok(segments.len())
    }

    /// Semantic search across stored segments.
    pub fn search(
        &self,
        query: &str,
        n: usize,
        video_id_filter: Option<&str>,
    ) -> Result<Vec<SearchResult>, StoreError> {
        if self.index.size() == 0 {
            return Ok(Vec::new());
        }

        let embeddings = embed_texts(&[query])?;
        let query_vec = &embeddings[0];

        let matches = match video_id_filter {
            Some(vid) => {
                let vid = vid.to_string();
                self.index
                    .filtered_search(query_vec, n, |key| {
                        self.metadata
                            .get(&key)
                            .is_some_and(|seg| seg.video_id == vid)
                    })
                    .map_err(|e| StoreError::Index(e.to_string()))?
            }
            None => self
                .index
                .search(query_vec, n)
                .map_err(|e| StoreError::Index(e.to_string()))?,
        };

        let results = matches
            .keys
            .iter()
            .zip(matches.distances.iter())
            .filter_map(|(&key, &distance)| {
                let seg = self.metadata.get(&key)?;
                Some(SearchResult {
                    video_id: seg.video_id.clone(),
                    start: seg.start,
                    end: seg.end,
                    text: seg.text.clone(),
                    distance,
                })
            })
            .collect();

        Ok(results)
    }

    /// Get all segments for a video, sorted by start time.
    pub fn get_segments(&self, video_id: &str) -> Result<Vec<StoredSegment>, StoreError> {
        let mut segments: Vec<StoredSegment> = self
            .metadata
            .values()
            .filter(|seg| seg.video_id == video_id)
            .cloned()
            .collect();

        if segments.is_empty() {
            return Err(StoreError::VideoNotFound(video_id.to_string()));
        }

        segments.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
        Ok(segments)
    }

    /// List all stored video IDs.
    pub fn get_video_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self
            .metadata
            .values()
            .map(|seg| seg.video_id.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        ids.sort();
        ids
    }

    /// Remove all segments for a video.
    pub fn delete_video(&mut self, video_id: &str) -> Result<usize, StoreError> {
        let keys_to_remove: Vec<u64> = self
            .metadata
            .values()
            .filter(|seg| seg.video_id == video_id)
            .map(|seg| seg.key)
            .collect();

        if keys_to_remove.is_empty() {
            return Err(StoreError::VideoNotFound(video_id.to_string()));
        }

        for key in &keys_to_remove {
            let _ = self.index.remove(*key);
            self.metadata.remove(key);
        }

        self.persist()?;
        Ok(keys_to_remove.len())
    }

    /// Write index and metadata to disk.
    fn persist(&self) -> Result<(), StoreError> {
        self.index
            .save(
                self.data_dir
                    .join(INDEX_FILE)
                    .to_str()
                    .unwrap_or_default(),
            )
            .map_err(|e| StoreError::Index(e.to_string()))?;

        let json = serde_json::to_string_pretty(&self.metadata)?;
        fs::write(self.data_dir.join(METADATA_FILE), json)?;

        Ok(())
    }
}
