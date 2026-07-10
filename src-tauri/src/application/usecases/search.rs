use std::collections::HashMap;
use std::sync::Arc;

use crate::domain::entities::chunk::SearchHit;
use crate::domain::entities::provider::ProviderKind;
use crate::domain::error::DomainResult;
use crate::domain::repositories::{DocumentRepository, ProviderConfigRepository};
use crate::domain::services::{ChatTurn, EmbeddingProvider, ProviderRouter, SettingsRepository};

/// Hybrid search: FTS5 keyword search fused with vector similarity using
/// Reciprocal Rank Fusion, optionally reranked by the local LLM. Falls back
/// to keyword-only when no embedder is reachable (fully offline default).
pub struct SearchUseCase {
    documents: Arc<dyn DocumentRepository>,
    embedder: Arc<dyn EmbeddingProvider>,
    router: Arc<dyn ProviderRouter>,
    providers: Arc<dyn ProviderConfigRepository>,
    settings: Arc<dyn SettingsRepository>,
}

const RRF_K: f64 = 60.0;
/// Rerank considers this many fused candidates.
const RERANK_POOL: usize = 12;

impl SearchUseCase {
    pub fn new(
        documents: Arc<dyn DocumentRepository>,
        embedder: Arc<dyn EmbeddingProvider>,
        router: Arc<dyn ProviderRouter>,
        providers: Arc<dyn ProviderConfigRepository>,
        settings: Arc<dyn SettingsRepository>,
    ) -> Self {
        Self {
            documents,
            embedder,
            router,
            providers,
            settings,
        }
    }

    pub async fn execute(
        &self,
        workspace_id: &str,
        query: &str,
        limit: usize,
    ) -> DomainResult<Vec<SearchHit>> {
        let pool = limit.max(RERANK_POOL);
        let keyword_hits = self
            .documents
            .search_keyword(workspace_id, query, pool * 2)?;

        let vector_ids = self.vector_search(workspace_id, query, pool * 2).await?;

        if vector_ids.is_empty() {
            let hits: Vec<SearchHit> = keyword_hits.into_iter().take(pool).collect();
            return Ok(self.maybe_rerank(query, hits, limit).await);
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
        let top_ids: Vec<String> = ranked.into_iter().take(pool).map(|(id, _)| id).collect();

        let mut hits = self.documents.hits_by_chunk_ids(&top_ids)?;
        // Preserve fusion order and reuse fused score.
        let order: HashMap<&String, usize> =
            top_ids.iter().enumerate().map(|(i, id)| (id, i)).collect();
        hits.sort_by_key(|h| *order.get(&h.chunk.id).unwrap_or(&usize::MAX));
        for (i, hit) in hits.iter_mut().enumerate() {
            hit.score = 1.0 / (i as f64 + 1.0);
        }
        Ok(self.maybe_rerank(query, hits, limit).await)
    }

    /// Optional LLM rerank (settings key `rerank_enabled`): the local model
    /// orders the fused candidates by relevance. Any failure degrades to the
    /// fusion order — search never breaks because reranking did.
    async fn maybe_rerank(
        &self,
        query: &str,
        hits: Vec<SearchHit>,
        limit: usize,
    ) -> Vec<SearchHit> {
        let enabled = self
            .settings
            .get("rerank_enabled")
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(false);
        if !enabled || hits.len() <= 2 {
            return hits.into_iter().take(limit).collect();
        }
        let Ok(Some(config)) = self.providers.find(ProviderKind::Ollama) else {
            return hits.into_iter().take(limit).collect();
        };
        let Ok(llm) = self.router.resolve(ProviderKind::Ollama) else {
            return hits.into_iter().take(limit).collect();
        };

        let mut prompt = format!(
            "Query: {query}\n\nRank the snippets below by relevance to the query. \
             Reply with ONLY the snippet numbers, comma-separated, most relevant first.\n\n"
        );
        for (i, hit) in hits.iter().enumerate() {
            let excerpt: String = hit.chunk.content.chars().take(300).collect();
            prompt.push_str(&format!("[{}] {}\n{}\n\n", i + 1, hit.rel_path, excerpt));
        }
        let turns = [ChatTurn::user(prompt)];
        let reply = match llm
            .chat(&config.default_model, "You are a code search reranker.", &turns)
            .await
        {
            Ok(reply) => reply,
            Err(_) => return hits.into_iter().take(limit).collect(),
        };

        let order = parse_rank_order(&reply, hits.len());
        if order.is_empty() {
            return hits.into_iter().take(limit).collect();
        }
        let mut reordered: Vec<SearchHit> = Vec::with_capacity(hits.len());
        let mut taken = vec![false; hits.len()];
        for index in order {
            if !taken[index] {
                taken[index] = true;
                reordered.push(hits[index].clone());
            }
        }
        for (i, hit) in hits.iter().enumerate() {
            if !taken[i] {
                reordered.push(hit.clone());
            }
        }
        for (i, hit) in reordered.iter_mut().enumerate() {
            hit.score = 1.0 / (i as f64 + 1.0);
        }
        reordered.into_iter().take(limit).collect()
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

/// Parse "3, 1, 2" (1-based) into 0-based indices, ignoring junk.
fn parse_rank_order(reply: &str, len: usize) -> Vec<usize> {
    reply
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<usize>().ok())
        .filter(|&n| n >= 1 && n <= len)
        .map(|n| n - 1)
        .collect()
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

    #[test]
    fn rank_order_parses_noisy_replies() {
        assert_eq!(super::parse_rank_order("3, 1, 2", 3), vec![2, 0, 1]);
        assert_eq!(
            super::parse_rank_order("Order: [2] then [1]. 9 is invalid.", 2),
            vec![1, 0]
        );
        assert!(super::parse_rank_order("no numbers here", 3).is_empty());
    }
}
