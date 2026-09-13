use crate::extraction::{ExtractionError, ExtractionQueue, ProposalRelation};
use crate::memory::{MemoryError, MemoryStore};
use crate::storage::{MemoryRecord, MemoryReviewStatus};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

pub const CONFLICT_EXCERPT_CHARS: usize = 280;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryConflictSide {
    pub id: String,
    pub memory_type: crate::storage::MemoryType,
    pub review_status: MemoryReviewStatus,
    pub locked: bool,
    pub updated_at: String,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryConflictPair {
    pub relation: ProposalRelation,
    pub from: MemoryConflictSide,
    pub to: MemoryConflictSide,
    pub proposal_id: String,
    pub created_at: String,
}

#[derive(Debug)]
pub enum ConflictError {
    Extraction(String),
    Memory(String),
    InvalidInput(String),
}

impl fmt::Display for ConflictError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Extraction(error) => write!(formatter, "extraction error: {error}"),
            Self::Memory(error) => write!(formatter, "memory error: {error}"),
            Self::InvalidInput(error) => write!(formatter, "invalid conflict input: {error}"),
        }
    }
}

impl std::error::Error for ConflictError {}

impl From<ExtractionError> for ConflictError {
    fn from(error: ExtractionError) -> Self {
        Self::Extraction(error.to_string())
    }
}

impl From<MemoryError> for ConflictError {
    fn from(error: MemoryError) -> Self {
        Self::Memory(error.to_string())
    }
}

impl From<ConflictError> for String {
    fn from(error: ConflictError) -> Self {
        error.to_string()
    }
}

pub fn list_conflict_pairs(
    queue: &ExtractionQueue,
    store: &mut MemoryStore,
) -> Result<Vec<MemoryConflictPair>, ConflictError> {
    let records = store
        .list()?
        .into_iter()
        .map(|record| (record.id.clone(), record))
        .collect::<HashMap<_, _>>();
    let mut pairs = Vec::new();
    for row in queue.list_relationships()? {
        let Some(pair) = conflict_pair_from_row(&row, &records) else {
            continue;
        };
        pairs.push(pair);
    }
    Ok(pairs)
}

pub fn exclude_conflict_side(
    queue: &ExtractionQueue,
    store: &mut MemoryStore,
    from_id: &str,
    to_id: &str,
    relation: &ProposalRelation,
    target_id: &str,
) -> Result<(), ConflictError> {
    match relation {
        ProposalRelation::Contradicts | ProposalRelation::Supersedes => {}
        ProposalRelation::Related => {
            return Err(ConflictError::InvalidInput(
                "related links are not conflicts".into(),
            ));
        }
    }
    if target_id != from_id && target_id != to_id {
        return Err(ConflictError::InvalidInput(
            "target must be one side of the conflict pair".into(),
        ));
    }
    if *relation == ProposalRelation::Supersedes && target_id != to_id {
        return Err(ConflictError::InvalidInput(
            "supersedes exclude may only target the older memory".into(),
        ));
    }
    if !queue.relationship_exists(from_id, to_id, relation)? {
        return Err(ConflictError::InvalidInput(
            "conflict pair is no longer open".into(),
        ));
    }
    let pairs = list_conflict_pairs(queue, store)?;
    let open = pairs
        .iter()
        .any(|pair| pair.relation == *relation && pair.from.id == from_id && pair.to.id == to_id);
    if !open {
        return Err(ConflictError::InvalidInput(
            "conflict pair is no longer open".into(),
        ));
    }
    let mut target = store
        .list()?
        .into_iter()
        .find(|memory| memory.id == target_id)
        .ok_or_else(|| ConflictError::InvalidInput("conflict target was not found".into()))?;
    target.review_status = MemoryReviewStatus::Excluded;
    store.update(&target)?;
    Ok(())
}

fn conflict_pair_from_row(
    row: &crate::extraction::MemoryRelationshipRow,
    records: &HashMap<String, MemoryRecord>,
) -> Option<MemoryConflictPair> {
    let relation = match row.relation.as_str() {
        "contradicts" => ProposalRelation::Contradicts,
        "supersedes" => ProposalRelation::Supersedes,
        _ => return None,
    };
    if row.from_memory_id == row.to_memory_id {
        return None;
    }
    let from = records.get(&row.from_memory_id)?;
    let to = records.get(&row.to_memory_id)?;
    if from.review_status == MemoryReviewStatus::Excluded
        || to.review_status == MemoryReviewStatus::Excluded
    {
        return None;
    }
    Some(MemoryConflictPair {
        relation,
        from: side_from_record(from),
        to: side_from_record(to),
        proposal_id: row.proposal_id.clone(),
        created_at: row.created_at.clone(),
    })
}

fn side_from_record(record: &MemoryRecord) -> MemoryConflictSide {
    MemoryConflictSide {
        id: record.id.clone(),
        memory_type: record.memory_type.clone(),
        review_status: record.review_status.clone(),
        locked: record.locked,
        updated_at: record.updated_at.clone(),
        excerpt: excerpt(&record.body),
    }
}

fn excerpt(body: &str) -> String {
    let collapsed = body.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= CONFLICT_EXCERPT_CHARS {
        collapsed
    } else {
        collapsed.chars().take(CONFLICT_EXCERPT_CHARS).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{new_stable_id, MemoryType, Vault};
    use std::path::PathBuf;

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("tz-chatter-conflicts-{label}-{}", new_stable_id()))
    }

    fn setup(label: &str) -> (PathBuf, Vault, ExtractionQueue, MemoryStore) {
        let root = root(label);
        let vault = Vault::create(&root).unwrap();
        let queue = ExtractionQueue::open(&vault, "lyra").unwrap();
        let store = MemoryStore::open(&vault, "lyra").unwrap();
        (root, vault, queue, store)
    }

    fn add_memory(store: &mut MemoryStore, id: &str, body: &str) -> MemoryRecord {
        let memory = MemoryRecord::new(id, MemoryType::Semantic, body);
        store.create(&memory).unwrap();
        memory
    }

    fn link(
        queue: &ExtractionQueue,
        from: &str,
        to: &str,
        relation: ProposalRelation,
        proposal_id: &str,
    ) {
        queue
            .record_relationship(from, to, &relation, proposal_id)
            .unwrap();
    }

    #[test]
    fn contradicts_pair_appears_with_excerpts() {
        let (root, _vault, queue, mut store) = setup("contradicts-list");
        add_memory(&mut store, "newer", "User now prefers busy cafes.");
        add_memory(&mut store, "older", "User prefers quiet cafes.");
        link(
            &queue,
            "newer",
            "older",
            ProposalRelation::Contradicts,
            "proposal-1",
        );
        let pairs = list_conflict_pairs(&queue, &mut store).unwrap();
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].relation, ProposalRelation::Contradicts);
        assert_eq!(pairs[0].from.id, "newer");
        assert_eq!(pairs[0].to.id, "older");
        assert_eq!(pairs[0].from.excerpt, "User now prefers busy cafes.");
        assert_eq!(pairs[0].to.excerpt, "User prefers quiet cafes.");
        drop(store);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn related_link_does_not_appear() {
        let (root, _vault, queue, mut store) = setup("related-hidden");
        add_memory(&mut store, "a", "Cafe lighting is warm.");
        add_memory(&mut store, "b", "The playlist is jazz.");
        link(
            &queue,
            "a",
            "b",
            ProposalRelation::Related,
            "proposal-related",
        );
        assert!(list_conflict_pairs(&queue, &mut store).unwrap().is_empty());
        drop(store);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn pair_disappears_when_either_side_is_excluded_or_deleted() {
        let (root, _vault, queue, mut store) = setup("leave-list");
        add_memory(&mut store, "from-a", "Prefers tea.");
        add_memory(&mut store, "to-a", "Prefers coffee.");
        add_memory(&mut store, "from-b", "Lives in Portland.");
        add_memory(&mut store, "to-b", "Lives in Seattle.");
        link(
            &queue,
            "from-a",
            "to-a",
            ProposalRelation::Contradicts,
            "p-a",
        );
        link(
            &queue,
            "from-b",
            "to-b",
            ProposalRelation::Contradicts,
            "p-b",
        );
        exclude_conflict_side(
            &queue,
            &mut store,
            "from-a",
            "to-a",
            &ProposalRelation::Contradicts,
            "to-a",
        )
        .unwrap();
        let after_exclude = list_conflict_pairs(&queue, &mut store).unwrap();
        assert_eq!(after_exclude.len(), 1);
        assert_eq!(after_exclude[0].from.id, "from-b");
        store.delete(&MemoryType::Semantic, "to-b").unwrap();
        assert!(list_conflict_pairs(&queue, &mut store).unwrap().is_empty());
        drop(store);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_side_is_omitted_without_error() {
        let (root, _vault, queue, mut store) = setup("missing-side");
        add_memory(&mut store, "newer", "Updated preference.");
        link(
            &queue,
            "newer",
            "ghost",
            ProposalRelation::Contradicts,
            "proposal-missing",
        );
        assert!(list_conflict_pairs(&queue, &mut store).unwrap().is_empty());
        drop(store);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn other_character_vault_links_are_not_returned() {
        let (root_a, _vault_a, queue_a, mut store_a) = setup("iso-a");
        let (root_b, _vault_b, queue_b, mut store_b) = setup("iso-b");
        add_memory(&mut store_a, "newer", "A prefers tea.");
        add_memory(&mut store_a, "older", "A prefers coffee.");
        link(
            &queue_a,
            "newer",
            "older",
            ProposalRelation::Contradicts,
            "proposal-a",
        );
        add_memory(&mut store_b, "other-new", "B prefers rain.");
        add_memory(&mut store_b, "other-old", "B prefers sun.");
        assert!(list_conflict_pairs(&queue_b, &mut store_b)
            .unwrap()
            .is_empty());
        drop(store_a);
        drop(queue_a);
        drop(store_b);
        drop(queue_b);
        std::fs::remove_dir_all(root_a).unwrap();
        std::fs::remove_dir_all(root_b).unwrap();
    }

    #[test]
    fn exclude_contradicts_side_drops_fts_and_keeps_file() {
        let (root, _vault, queue, mut store) = setup("exclude-contradicts");
        add_memory(
            &mut store,
            "newer",
            "User now prefers busy cafes busycafezzz.",
        );
        add_memory(
            &mut store,
            "older",
            "User prefers quiet cafes quietcafezzz.",
        );
        link(
            &queue,
            "newer",
            "older",
            ProposalRelation::Contradicts,
            "proposal-1",
        );
        exclude_conflict_side(
            &queue,
            &mut store,
            "newer",
            "older",
            &ProposalRelation::Contradicts,
            "older",
        )
        .unwrap();
        assert!(list_conflict_pairs(&queue, &mut store).unwrap().is_empty());
        let listed = store.list().unwrap();
        let older = listed.iter().find(|memory| memory.id == "older").unwrap();
        let newer = listed.iter().find(|memory| memory.id == "newer").unwrap();
        assert_eq!(older.review_status, MemoryReviewStatus::Excluded);
        assert_eq!(newer.review_status, MemoryReviewStatus::Accepted);
        assert_eq!(older.body, "User prefers quiet cafes quietcafezzz.");
        assert!(store.search("quietcafezzz", 10).unwrap().is_empty());
        assert_eq!(store.search("busycafezzz", 10).unwrap().len(), 1);
        drop(store);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn supersedes_exclude_older_rejects_newer_on_this_command() {
        let (root, _vault, queue, mut store) = setup("supersedes-older");
        add_memory(&mut store, "newer", "Current city is Portland.");
        add_memory(&mut store, "older", "Current city is Seattle.");
        link(
            &queue,
            "newer",
            "older",
            ProposalRelation::Supersedes,
            "proposal-s",
        );
        let err = exclude_conflict_side(
            &queue,
            &mut store,
            "newer",
            "older",
            &ProposalRelation::Supersedes,
            "newer",
        )
        .unwrap_err();
        assert!(err.to_string().contains("older memory"));
        assert_eq!(list_conflict_pairs(&queue, &mut store).unwrap().len(), 1);
        exclude_conflict_side(
            &queue,
            &mut store,
            "newer",
            "older",
            &ProposalRelation::Supersedes,
            "older",
        )
        .unwrap();
        assert!(list_conflict_pairs(&queue, &mut store).unwrap().is_empty());
        let older = store
            .list()
            .unwrap()
            .into_iter()
            .find(|memory| memory.id == "older")
            .unwrap();
        assert_eq!(older.review_status, MemoryReviewStatus::Excluded);
        drop(store);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn locked_older_side_can_be_excluded() {
        let (root, _vault, queue, mut store) = setup("locked-exclude");
        add_memory(&mut store, "newer", "New preference");
        let mut older = MemoryRecord::new("older", MemoryType::Semantic, "Old preference");
        older.locked = true;
        store.create(&older).unwrap();
        link(
            &queue,
            "newer",
            "older",
            ProposalRelation::Supersedes,
            "proposal-lock",
        );
        exclude_conflict_side(
            &queue,
            &mut store,
            "newer",
            "older",
            &ProposalRelation::Supersedes,
            "older",
        )
        .unwrap();
        let older = store
            .list()
            .unwrap()
            .into_iter()
            .find(|memory| memory.id == "older")
            .unwrap();
        assert!(older.locked);
        assert_eq!(older.review_status, MemoryReviewStatus::Excluded);
        drop(store);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn exclude_errors_when_pair_is_already_gone() {
        let (root, _vault, queue, mut store) = setup("already-gone");
        add_memory(&mut store, "newer", "New fact");
        add_memory(&mut store, "older", "Old fact");
        link(
            &queue,
            "newer",
            "older",
            ProposalRelation::Contradicts,
            "proposal-gone",
        );
        exclude_conflict_side(
            &queue,
            &mut store,
            "newer",
            "older",
            &ProposalRelation::Contradicts,
            "older",
        )
        .unwrap();
        let err = exclude_conflict_side(
            &queue,
            &mut store,
            "newer",
            "older",
            &ProposalRelation::Contradicts,
            "older",
        )
        .unwrap_err();
        assert!(err.to_string().contains("no longer open"));
        drop(store);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn list_command_does_not_write() {
        let (root, vault, queue, mut store) = setup("list-readonly");
        add_memory(&mut store, "newer", "New fact");
        add_memory(&mut store, "older", "Old fact");
        link(
            &queue,
            "newer",
            "older",
            ProposalRelation::Contradicts,
            "proposal-ro",
        );
        let before = queue.list_relationships().unwrap();
        let before_body =
            std::fs::read(vault.memory_path(&MemoryType::Semantic, "older").unwrap()).unwrap();
        let _ = list_conflict_pairs(&queue, &mut store).unwrap();
        let after = queue.list_relationships().unwrap();
        let after_body =
            std::fs::read(vault.memory_path(&MemoryType::Semantic, "older").unwrap()).unwrap();
        assert_eq!(before, after);
        assert_eq!(before_body, after_body);
        drop(store);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }
}
