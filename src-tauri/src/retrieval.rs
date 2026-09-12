use crate::{
    embeddings::SemanticSearchResult,
    memory::{MemoryError, MemorySearchResult, MemoryStore},
    prompt::estimate_tokens,
    storage::MemoryRecord,
};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetrievalBudget {
    pub max_results: usize,
    pub max_tokens: usize,
    pub pinned_tokens: usize,
}

impl Default for RetrievalBudget {
    fn default() -> Self {
        Self {
            max_results: 6,
            max_tokens: 512,
            pinned_tokens: 192,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RetrievalReason {
    LexicalMatch,
    SemanticSimilarity,
    EntityMatch,
    RelatedMemory,
    Pinned,
    Salience,
    Recency,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RetrievedMemory {
    pub memory_id: String,
    pub source_path: String,
    pub content: String,
    pub salience: f32,
    pub pinned: bool,
    pub estimated_tokens: usize,
    pub reasons: Vec<RetrievalReason>,
    pub lexical_score: Option<f32>,
    pub semantic_score: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RetrievalReport {
    pub selected: Vec<RetrievedMemory>,
    pub candidate_count: usize,
    pub omitted_count: usize,
}

pub fn retrieve(
    store: &mut MemoryStore,
    query: &str,
    budget: RetrievalBudget,
) -> Result<Vec<RetrievedMemory>, MemoryError> {
    Ok(retrieve_with_report(store, query, budget)?.selected)
}

pub fn retrieve_with_report(
    store: &mut MemoryStore,
    query: &str,
    budget: RetrievalBudget,
) -> Result<RetrievalReport, MemoryError> {
    if budget.max_results == 0 || budget.max_tokens == 0 {
        return Ok(RetrievalReport {
            selected: Vec::new(),
            candidate_count: 0,
            omitted_count: 0,
        });
    }
    let candidates = store.search(query, budget.max_results.saturating_mul(4).max(1))?;
    let mut unique = Vec::new();
    let mut seen = HashSet::new();
    for candidate in candidates {
        if seen.insert(candidate.memory_id.clone()) {
            unique.push(candidate);
        }
    }
    unique.sort_by(compare_candidates);
    let candidate_count = unique.len();

    let mut selected = Vec::new();
    let mut total_tokens = 0;
    let mut pinned_tokens = 0;
    for (rank, candidate) in unique.into_iter().enumerate() {
        let reasons = lexical_reasons(&candidate, rank);
        let estimated_tokens = estimate_tokens(&format!(
            "[{}] {}\n{}",
            candidate.source_path, candidate.memory_id, candidate.chunk
        ));
        if candidate.pinned {
            if pinned_tokens + estimated_tokens > budget.pinned_tokens {
                continue;
            }
            pinned_tokens += estimated_tokens;
        }
        if total_tokens + estimated_tokens > budget.max_tokens {
            continue;
        }
        selected.push(RetrievedMemory {
            memory_id: candidate.memory_id,
            source_path: candidate.source_path,
            content: candidate.chunk,
            salience: candidate.salience,
            pinned: candidate.pinned,
            estimated_tokens,
            reasons,
            lexical_score: Some(rank_score(rank)),
            semantic_score: None,
        });
        total_tokens += estimated_tokens;
        if selected.len() >= budget.max_results {
            break;
        }
    }
    let omitted_count = candidate_count.saturating_sub(selected.len());
    Ok(RetrievalReport {
        selected,
        candidate_count,
        omitted_count,
    })
}

#[derive(Debug, Clone)]
struct HybridCandidate {
    memory_id: String,
    source_path: String,
    content: String,
    salience: f32,
    pinned: bool,
    updated_at: String,
    lexical_rank: Option<usize>,
    semantic_score: Option<f32>,
    entity_match: bool,
}

#[derive(Debug, Clone)]
struct RankedCandidate {
    candidate: HybridCandidate,
    score: f32,
    recency_rank: usize,
}

/// Combines lexical and semantic candidates while retaining the existing lexical-only path as the safe default.
pub fn hybrid_retrieve(
    store: &mut MemoryStore,
    query: &str,
    semantic_candidates: &[SemanticSearchResult],
    related_memory_ids: &[String],
    budget: RetrievalBudget,
) -> Result<Vec<RetrievedMemory>, MemoryError> {
    if budget.max_results == 0 || budget.max_tokens == 0 {
        return Ok(Vec::new());
    }

    let lexical_candidates = store.search(query, budget.max_results.saturating_mul(8).max(1))?;
    let records = store.list()?;
    let metadata = records
        .into_iter()
        .map(|record| (record.id.clone(), record))
        .collect::<HashMap<_, _>>();
    let related = related_memory_ids.iter().collect::<HashSet<_>>();
    let query_terms = normalized_terms(query);
    let mut candidates = HashMap::<String, HybridCandidate>::new();

    for (rank, lexical) in lexical_candidates.into_iter().enumerate() {
        let entry = candidates
            .entry(lexical.memory_id.clone())
            .or_insert_with(|| candidate_from_search(&lexical));
        entry.lexical_rank = Some(entry.lexical_rank.map_or(rank, |old| old.min(rank)));
        entry.content = lexical.chunk;
        entry.entity_match = entity_match(&query_terms, &entry.content, &entry.memory_id);
    }

    for semantic in semantic_candidates {
        let Some(record) = metadata.get(&semantic.memory_id) else {
            continue;
        };
        let entry = candidates
            .entry(semantic.memory_id.clone())
            .or_insert_with(|| candidate_from_record(record));
        if entry
            .semantic_score
            .is_none_or(|score| semantic.score > score)
        {
            entry.semantic_score = Some(semantic.score);
            entry.content = semantic.content.clone();
        }
        entry.entity_match = entity_match(&query_terms, &entry.content, &entry.memory_id);
    }

    let mut recency_order = candidates.values().collect::<Vec<_>>();
    recency_order.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| left.memory_id.cmp(&right.memory_id))
    });
    let recency_ranks = recency_order
        .iter()
        .enumerate()
        .map(|(rank, candidate)| (candidate.memory_id.clone(), rank))
        .collect::<HashMap<_, _>>();

    let mut ranked = candidates
        .into_values()
        .map(|candidate| {
            let recency_rank = recency_ranks[&candidate.memory_id];
            let lexical = candidate.lexical_rank.map(rank_score).unwrap_or(0.0);
            let semantic = candidate.semantic_score.map(semantic_score).unwrap_or(0.0);
            let salience = candidate.salience.clamp(0.0, 1.0);
            let recency = rank_score(recency_rank);
            let related_bonus = if related.contains(&candidate.memory_id) {
                0.10
            } else {
                0.0
            };
            let entity_bonus = if candidate.entity_match { 0.08 } else { 0.0 };
            let pinned_bonus = if candidate.pinned { 0.08 } else { 0.0 };
            let score = 0.42 * lexical
                + 0.34 * semantic
                + 0.08 * salience
                + 0.05 * recency
                + related_bonus
                + entity_bonus
                + pinned_bonus;
            RankedCandidate {
                candidate,
                score,
                recency_rank,
            }
        })
        .collect::<Vec<_>>();

    let mut selected = Vec::new();
    let mut type_counts = HashMap::<String, usize>::new();
    let mut total_tokens = 0;
    let mut pinned_tokens = 0;
    while !ranked.is_empty() && selected.len() < budget.max_results {
        let best_index = ranked
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| {
                let left_adjusted = left.score
                    - 0.06 * (*type_counts.get(type_key(&left.candidate)).unwrap_or(&0) as f32);
                let right_adjusted = right.score
                    - 0.06 * (*type_counts.get(type_key(&right.candidate)).unwrap_or(&0) as f32);
                right_adjusted
                    .partial_cmp(&left_adjusted)
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| {
                        right
                            .score
                            .partial_cmp(&left.score)
                            .unwrap_or(Ordering::Equal)
                    })
                    .then_with(|| left.candidate.memory_id.cmp(&right.candidate.memory_id))
            })
            .map(|(index, _)| index)
            .expect("ranked is non-empty");
        let ranked_candidate = ranked.remove(best_index);
        let candidate = ranked_candidate.candidate;
        let estimated_tokens = estimate_tokens(&format!(
            "[{}] {}\n{}",
            candidate.source_path, candidate.memory_id, candidate.content
        ));
        if candidate.pinned {
            if pinned_tokens + estimated_tokens > budget.pinned_tokens {
                continue;
            }
            pinned_tokens += estimated_tokens;
        }
        if total_tokens + estimated_tokens > budget.max_tokens {
            continue;
        }

        let mut reasons = Vec::new();
        if candidate.lexical_rank.is_some() {
            reasons.push(RetrievalReason::LexicalMatch);
        }
        if candidate.semantic_score.is_some() {
            reasons.push(RetrievalReason::SemanticSimilarity);
        }
        if candidate.entity_match {
            reasons.push(RetrievalReason::EntityMatch);
        }
        if related.contains(&candidate.memory_id) {
            reasons.push(RetrievalReason::RelatedMemory);
        }
        if candidate.pinned {
            reasons.push(RetrievalReason::Pinned);
        }
        if candidate.salience >= 0.7 {
            reasons.push(RetrievalReason::Salience);
        }
        if ranked_candidate.recency_rank == 0 {
            reasons.push(RetrievalReason::Recency);
        }
        let type_name = type_key(&candidate).to_string();
        *type_counts.entry(type_name).or_default() += 1;
        total_tokens += estimated_tokens;
        selected.push(RetrievedMemory {
            memory_id: candidate.memory_id,
            source_path: candidate.source_path,
            content: candidate.content,
            salience: candidate.salience,
            pinned: candidate.pinned,
            estimated_tokens,
            reasons,
            lexical_score: candidate.lexical_rank.map(rank_score),
            semantic_score: candidate.semantic_score.map(semantic_score),
        });
    }
    Ok(selected)
}

fn candidate_from_search(candidate: &MemorySearchResult) -> HybridCandidate {
    HybridCandidate {
        memory_id: candidate.memory_id.clone(),
        source_path: candidate.source_path.clone(),
        content: candidate.chunk.clone(),
        salience: candidate.salience,
        pinned: candidate.pinned,
        updated_at: candidate.updated_at.clone(),
        lexical_rank: None,
        semantic_score: None,
        entity_match: false,
    }
}

fn candidate_from_record(record: &MemoryRecord) -> HybridCandidate {
    HybridCandidate {
        memory_id: record.id.clone(),
        source_path: MemoryStore::source_path_for(&record.memory_type, &record.id),
        content: record.body.clone(),
        salience: record.salience,
        pinned: record.pinned,
        updated_at: record.updated_at.clone(),
        lexical_rank: None,
        semantic_score: None,
        entity_match: false,
    }
}

fn normalized_terms(value: &str) -> HashSet<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(str::to_lowercase)
        .collect()
}

fn entity_match(query_terms: &HashSet<String>, content: &str, memory_id: &str) -> bool {
    let content_terms = normalized_terms(content);
    let id_terms = normalized_terms(memory_id);
    query_terms
        .iter()
        .any(|term| content_terms.contains(term) || id_terms.contains(term))
}

fn semantic_score(score: f32) -> f32 {
    ((score + 1.0) / 2.0).clamp(0.0, 1.0)
}

fn rank_score(rank: usize) -> f32 {
    1.0 / (rank as f32 + 1.0)
}

fn type_key(candidate: &HybridCandidate) -> &str {
    candidate.source_path.split('/').nth(1).unwrap_or("unknown")
}

fn lexical_reasons(candidate: &MemorySearchResult, rank: usize) -> Vec<RetrievalReason> {
    let mut reasons = vec![RetrievalReason::LexicalMatch];
    if candidate.pinned {
        reasons.push(RetrievalReason::Pinned);
    }
    if candidate.salience >= 0.7 {
        reasons.push(RetrievalReason::Salience);
    }
    if rank == 0 {
        reasons.push(RetrievalReason::Recency);
    }
    reasons
}

fn compare_candidates(left: &MemorySearchResult, right: &MemorySearchResult) -> Ordering {
    right
        .pinned
        .cmp(&left.pinned)
        .then_with(|| {
            right
                .salience
                .partial_cmp(&left.salience)
                .unwrap_or(Ordering::Equal)
        })
        .then_with(|| right.updated_at.cmp(&left.updated_at))
        .then_with(|| {
            left.score
                .partial_cmp(&right.score)
                .unwrap_or(Ordering::Equal)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{MemoryRecord, MemoryType, Vault};
    use std::{fs, path::PathBuf, time::Instant};

    fn root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "tz-chatter-retrieval-{}",
            crate::storage::new_stable_id()
        ))
    }

    fn memory(id: &str, body: &str, salience: f32, pinned: bool) -> MemoryRecord {
        let mut value = MemoryRecord::new(id, MemoryType::Semantic, body);
        value.created_at = "1".into();
        value.updated_at = id.into();
        value.salience = salience;
        value.pinned = pinned;
        value
    }

    fn evaluation_memory(
        id: &str,
        body: &str,
        memory_type: MemoryType,
        salience: f32,
        updated_at: &str,
    ) -> MemoryRecord {
        let mut value = MemoryRecord::new(id, memory_type, body);
        value.created_at = updated_at.into();
        value.updated_at = updated_at.into();
        value.salience = salience;
        value
    }

    #[test]
    fn retrieves_relevant_memories_once_with_source_labels_and_budgets() {
        let root = root();
        let vault = Vault::create(&root).unwrap();
        let mut store = MemoryStore::open(&vault, "lyra").unwrap();
        store
            .create(&memory("pinned", "Mina drinks tea", 0.4, true))
            .unwrap();
        store
            .create(&memory("ordinary", "Mina visits cafes", 0.9, false))
            .unwrap();
        let selected = retrieve(
            &mut store,
            "Mina",
            RetrievalBudget {
                max_results: 4,
                max_tokens: 100,
                pinned_tokens: 100,
            },
        )
        .unwrap();
        assert_eq!(selected.len(), 2);
        assert_eq!(selected[0].memory_id, "pinned");
        assert!(selected[0].source_path.contains("memories/semantic/"));
        assert!(selected.iter().all(|memory| memory.estimated_tokens > 0));
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn natural_language_queries_still_retrieve_matching_memories() {
        let root = root();
        let vault = Vault::create(&root).unwrap();
        let mut store = MemoryStore::open(&vault, "lyra").unwrap();
        store
            .create(&memory("fact", "Mina likes tea", 0.8, false))
            .unwrap();
        let selected = retrieve(
            &mut store,
            "Do you remember whether Mina likes coffee?",
            RetrievalBudget::default(),
        )
        .unwrap();
        assert!(selected.iter().any(|memory| memory.memory_id == "fact"));
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pinned_budget_and_max_tokens_bound_selection() {
        let root = root();
        let vault = Vault::create(&root).unwrap();
        let mut store = MemoryStore::open(&vault, "lyra").unwrap();
        store
            .create(&memory("pinned", "Mina drinks tea", 0.4, true))
            .unwrap();
        store
            .create(&memory("ordinary", "Mina visits cafes", 0.9, false))
            .unwrap();
        let selected = retrieve(
            &mut store,
            "Mina",
            RetrievalBudget {
                max_results: 4,
                max_tokens: 12,
                pinned_tokens: 1,
            },
        )
        .unwrap();
        assert!(selected.iter().all(|memory| !memory.pinned));
        assert!(
            selected
                .iter()
                .map(|memory| memory.estimated_tokens)
                .sum::<usize>()
                <= 12
        );
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hybrid_evaluation_covers_paraphrases_names_stale_records_and_isolation() {
        let root = root();
        let vault = Vault::create(&root).unwrap();
        let mut store = MemoryStore::open(&vault, "lyra").unwrap();
        store
            .create(&evaluation_memory(
                "mina-name",
                "Mina prefers tea with Juniper.",
                MemoryType::People,
                0.7,
                "2026-09-10T10:00:00Z",
            ))
            .unwrap();
        store
            .create(&evaluation_memory(
                "quiet-cafes",
                "Mina enjoys quiet coffee shops near the river.",
                MemoryType::Semantic,
                0.9,
                "2026-09-11T10:00:00Z",
            ))
            .unwrap();
        store
            .create(&evaluation_memory(
                "stale-bars",
                "Mina used to prefer loud bars, before the move.",
                MemoryType::Episodic,
                0.2,
                "2022-01-01T10:00:00Z",
            ))
            .unwrap();
        store
            .create(&evaluation_memory(
                "distractor",
                "Juniper collects pressed flowers.",
                MemoryType::Relationships,
                0.8,
                "2026-09-11T11:00:00Z",
            ))
            .unwrap();

        let semantic = vec![
            SemanticSearchResult {
                memory_id: "quiet-cafes".into(),
                fingerprint: "1:quiet".into(),
                content: "Mina enjoys quiet coffee shops near the river.".into(),
                score: 0.98,
            },
            SemanticSearchResult {
                memory_id: "stale-bars".into(),
                fingerprint: "1:stale".into(),
                content: "Mina used to prefer loud bars, before the move.".into(),
                score: 0.62,
            },
            SemanticSearchResult {
                memory_id: "other-character-only".into(),
                fingerprint: "1:foreign".into(),
                content: "A foreign character likes calm espresso.".into(),
                score: 0.99,
            },
        ];
        let related = vec!["quiet-cafes".to_string()];
        let budget = RetrievalBudget {
            max_results: 4,
            max_tokens: 256,
            pinned_tokens: 64,
        };

        let lexical_start = Instant::now();
        let lexical = retrieve(&mut store, "Mina", budget).unwrap();
        let lexical_elapsed = lexical_start.elapsed();
        let hybrid_start = Instant::now();
        let hybrid = hybrid_retrieve(&mut store, "Mina", &semantic, &related, budget).unwrap();
        let hybrid_elapsed = hybrid_start.elapsed();
        let paraphrase =
            hybrid_retrieve(&mut store, "calm espresso", &semantic, &related, budget).unwrap();

        assert!(lexical.iter().any(|item| item.memory_id == "mina-name"));
        assert!(hybrid.iter().any(|item| item.memory_id == "mina-name"));
        assert!(hybrid.iter().any(|item| item.memory_id == "quiet-cafes"));
        assert_eq!(
            paraphrase.first().map(|item| item.memory_id.as_str()),
            Some("quiet-cafes")
        );
        assert!(paraphrase[0]
            .reasons
            .contains(&RetrievalReason::SemanticSimilarity));
        assert!(paraphrase[0]
            .reasons
            .contains(&RetrievalReason::RelatedMemory));
        assert!(!hybrid
            .iter()
            .any(|item| item.memory_id == "other-character-only"));
        assert!(hybrid
            .iter()
            .position(|item| item.memory_id == "stale-bars")
            .map(|stale| stale > 0)
            .unwrap_or(true));
        eprintln!(
            "T14 retrieval evaluation: lexical_recall_exact={} hybrid_recall_exact_and_paraphrase={} lexical_us={} hybrid_us={}",
            lexical.iter().any(|item| item.memory_id == "mina-name"),
            hybrid.iter().any(|item| item.memory_id == "mina-name")
                && hybrid.iter().any(|item| item.memory_id == "quiet-cafes"),
            lexical_elapsed.as_micros(),
            hybrid_elapsed.as_micros()
        );
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }
}
