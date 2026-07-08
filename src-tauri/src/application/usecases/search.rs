use std::collections::HashMap;
use std::sync::Arc;

use crate::domain::entities::chunk::SearchHit;
use crate::domain::error::DomainResult;
use crate::domain::repositories::DocumentRepository;
use crate::domain::services::EmbeddingProvider;

/// Hybrid search: FTS5 keyword search fused with vector similarity using
/// Reciprocal Rank Fusion. Falls back to keyword-only when no embedder is
/// reachable (fully offline default).
pub struct SearchUseCase {
    documents: Arc<dyn DocumentRepository>,
    embedder: Arc<dyn EmbeddingProvider>,
}

const RRF_K: f64 = 60.0;

impl SearchUseCase {
    pub fn new(
        documents: Arc<dyn DocumentRepository>,
        embedder: Arc<dyn EmbeddingProvider>,
    ) -> Self {
        Self {
            documents,
            embedder,
        }
    }

    pub async fn execute(
        &self,
        workspace_id: &str,
        query: &str,
        limit: usize,
    ) -> DomainResult<Vec<SearchHit>> {
        let keyword_hits = self
            .documents
            .search_keyword(workspace_id, query, limit * 2)?;

        let vector_ids = self.vector_search(workspace_id, query, limit * 2).await?;

        if vector_ids.is_empty() {
            return Ok(keyword_hits.into_iter().take(limit).collect());
        }

        // Reciprocal Rank Fusion over both ranked lists.
        let mut scores: HashMap<String, f64> = HashMap::new();
        for (rank, hit) in keyword_hits.iter().enumerate() {
            *scores.entry(hit.chunk.id.clone()).or_default() +=
                1.0 / (RRF_K + rank as f64 + 1.0);
        }
        for (rank, chunk_id) in vector_ids.iter().enumerate() {
            *scores.entry(chunk_id.clone()).or_default() += 1.0 / (RRF_K + rank as f64 + 1.0);
        }

        let mut ranked: Vec<(String, f64)> = scores.into_iter().collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let top_ids: Vec<String> = ranked.into_iter().take(limit).map(|(id, _)| id).collect();

        let mut hits = self.documents.hits_by_chunk_ids(&top_ids)?;
        // Preserve fusion order and reuse fused score.
        let order: HashMap<&String, usize> =
            top_ids.iter().enumerate().map(|(i, id)| (id, i)).collect();
        hits.sort_by_key(|h| *order.get(&h.chunk.id).unwrap_or(&usize::MAX));
        for (i, hit) in hits.iter_mut().enumerate() {
            hit.score = 1.0 / (i as f64 + 1.0);
        }
        Ok(hits)
    }

    async fn vector_search(
        &self,
        workspace_id: &str,
        query: &str,
        limit: usize,
    ) -> DomainResult<Vec<String>> {
        if !self.embedder.is_available().await {
            return Ok(Vec::new());
        }
        let query_vec = match self.embedder.embed(&[query.to_string()]).await {
            Ok(mut v) if !v.is_empty() => v.remove(0),
            _ => return Ok(Vec::new()),
        };
        let embeddings = self.documents.embeddings_by_workspace(workspace_id)?;
        let mut scored: Vec<(String, f64)> = embeddings
            .into_iter()
            .map(|(id, vec)| (id, cosine_similarity(&query_vec, &vec)))
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(scored.into_iter().take(limit).map(|(id, _)| id).collect())
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0.0f64, 0.0f64, 0.0f64);
    for (x, y) in a.iter().zip(b.iter()) {
        dot += (*x as f64) * (*y as f64);
        na += (*x as f64) * (*x as f64);
        nb += (*y as f64) * (*y as f64);
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

#[cfg(test)]
mod tests {
    use super::cosine_similarity;

    #[test]
    fn cosine_of_identical_vectors_is_one() {
        let v = vec![0.5f32, 0.25, -0.3];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn cosine_of_orthogonal_vectors_is_zero() {
        assert_eq!(cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]), 0.0);
    }

    #[test]
    fn mismatched_lengths_are_zero() {
        assert_eq!(cosine_similarity(&[1.0], &[1.0, 2.0]), 0.0);
    }
}
