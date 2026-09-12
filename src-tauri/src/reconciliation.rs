use crate::extraction::{
    ExtractionError, ExtractionQueue, MemoryProposal, ProposalRelation, ProposalStatus,
};
use crate::memory::{MemoryError, MemoryStore};
use crate::storage::{new_stable_id, MemoryRecord, MemoryReviewStatus};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommitResult {
    Committed { memory_id: String },
    Duplicate { memory_id: String },
    Suppressed,
    Locked { memory_id: String },
}

#[derive(Debug)]
pub enum ReconciliationError {
    Extraction(String),
    Memory(String),
    InvalidInput(String),
}

impl fmt::Display for ReconciliationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Extraction(error) => write!(formatter, "extraction error: {error}"),
            Self::Memory(error) => write!(formatter, "memory error: {error}"),
            Self::InvalidInput(error) => write!(formatter, "invalid reconciliation input: {error}"),
        }
    }
}

impl std::error::Error for ReconciliationError {}

impl From<ExtractionError> for ReconciliationError {
    fn from(error: ExtractionError) -> Self {
        Self::Extraction(error.to_string())
    }
}

impl From<MemoryError> for ReconciliationError {
    fn from(error: MemoryError) -> Self {
        Self::Memory(error.to_string())
    }
}

pub struct ReconciliationService<'a> {
    queue: &'a ExtractionQueue,
    memories: &'a mut MemoryStore,
}

impl<'a> ReconciliationService<'a> {
    pub fn new(queue: &'a ExtractionQueue, memories: &'a mut MemoryStore) -> Self {
        Self { queue, memories }
    }

    pub fn commit_accepted(
        &mut self,
        proposal_id: &str,
    ) -> Result<CommitResult, ReconciliationError> {
        let proposal = self
            .queue
            .get_proposal(proposal_id)?
            .ok_or_else(|| ReconciliationError::InvalidInput("proposal not found".into()))?;
        match proposal.status {
            ProposalStatus::Committed => {
                if let Some(existing) = self.find_duplicate(&proposal)? {
                    return Ok(CommitResult::Duplicate {
                        memory_id: existing.id,
                    });
                }
                return Err(ReconciliationError::InvalidInput(
                    "committed proposal has no matching memory".into(),
                ));
            }
            ProposalStatus::Accepted => {}
            ProposalStatus::NeedsReview => {
                return Err(ReconciliationError::InvalidInput(
                    "proposal must be accepted before commit".into(),
                ));
            }
            ProposalStatus::Rejected | ProposalStatus::Superseded => {
                return Err(ReconciliationError::InvalidInput(
                    "proposal is not eligible for commit".into(),
                ));
            }
        }

        if self.queue.is_suppressed(
            &proposal.source_session_id,
            &proposal.candidate.source_turn_ids,
            &proposal.candidate.memory_type,
        )? {
            self.queue
                .mark_proposal(proposal_id, ProposalStatus::Superseded)?;
            return Ok(CommitResult::Suppressed);
        }

        let existing = self.memories.list()?;
        for related_id in &proposal.candidate.related_memory_ids {
            if let Some(memory) = existing.iter().find(|memory| &memory.id == related_id) {
                if memory.locked
                    && proposal.candidate.relation == Some(ProposalRelation::Supersedes)
                {
                    return Ok(CommitResult::Locked {
                        memory_id: memory.id.clone(),
                    });
                }
            }
        }

        if let Some(duplicate) = existing.iter().find(|memory| {
            memory.memory_type == proposal.candidate.memory_type
                && normalized(&memory.body) == normalized(&proposal.candidate.body)
        }) {
            self.record_relationships(&proposal, &duplicate.id)?;
            self.queue
                .mark_proposal(proposal_id, ProposalStatus::Committed)?;
            return Ok(CommitResult::Duplicate {
                memory_id: duplicate.id.clone(),
            });
        }

        let timestamp = now();
        let mut memory = MemoryRecord::new(
            new_stable_id(),
            proposal.candidate.memory_type.clone(),
            proposal.candidate.body.clone(),
        );
        memory.created_at = timestamp.clone();
        memory.updated_at = timestamp;
        memory.source_session_id = Some(proposal.source_session_id.clone());
        memory.source_turn_ids = proposal.candidate.source_turn_ids.clone();
        memory.confidence = proposal.candidate.confidence;
        memory.salience = proposal.candidate.confidence;
        memory.review_status = MemoryReviewStatus::Accepted;

        // MemoryStore writes the authoritative Markdown first, then refreshes the
        // rebuildable index. If index refresh fails, the committed Markdown remains
        // available for a later rebuild and this proposal stays retryable.
        self.memories.create(&memory)?;
        self.record_relationships(&proposal, &memory.id)?;
        self.queue
            .mark_proposal(proposal_id, ProposalStatus::Committed)?;
        Ok(CommitResult::Committed {
            memory_id: memory.id,
        })
    }

    pub fn reject(&self, proposal_id: &str) -> Result<(), ReconciliationError> {
        let proposal = self
            .queue
            .get_proposal(proposal_id)?
            .ok_or_else(|| ReconciliationError::InvalidInput("proposal not found".into()))?;
        if proposal.status != ProposalStatus::NeedsReview {
            return Err(ReconciliationError::InvalidInput(
                "only a pending proposal can be rejected".into(),
            ));
        }
        self.queue
            .mark_proposal(proposal_id, ProposalStatus::Rejected)?;
        Ok(())
    }

    pub fn repair_index(&mut self) -> Result<(), ReconciliationError> {
        self.memories.rebuild()?;
        Ok(())
    }

    fn find_duplicate(
        &mut self,
        proposal: &MemoryProposal,
    ) -> Result<Option<MemoryRecord>, ReconciliationError> {
        Ok(self.memories.list()?.into_iter().find(|memory| {
            memory.memory_type == proposal.candidate.memory_type
                && normalized(&memory.body) == normalized(&proposal.candidate.body)
        }))
    }

    fn record_relationships(
        &self,
        proposal: &MemoryProposal,
        new_memory_id: &str,
    ) -> Result<(), ReconciliationError> {
        if let Some(relation) = &proposal.candidate.relation {
            for related_id in &proposal.candidate.related_memory_ids {
                self.queue.record_relationship(
                    new_memory_id,
                    related_id,
                    relation,
                    &proposal.id,
                )?;
            }
        }
        Ok(())
    }
}

fn normalized(body: &str) -> String {
    body.split_whitespace()
        .map(|part| part.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

fn now() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::{
        ExtractionCandidate, ExtractionQueue, ProposalEvidence, ProposalOrigin,
    };
    use crate::memory::MemoryStore;
    use crate::storage::{
        new_stable_id, MemoryType, TranscriptDocument, TranscriptTurn, TurnRole, TurnStatus, Vault,
    };
    use std::path::PathBuf;

    fn root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("tz-chatter-reconcile-{label}-{}", new_stable_id()))
    }

    fn transcript() -> TranscriptDocument {
        let mut transcript = TranscriptDocument::new("session-1", "lyra");
        transcript.turns.push(TranscriptTurn {
            id: "user-1".into(),
            timestamp: "1".into(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: "I like tea.".into(),
        });
        transcript.turns.push(TranscriptTurn {
            id: "assistant-1".into(),
            timestamp: "2".into(),
            role: TurnRole::Assistant,
            status: TurnStatus::Complete,
            content: "I will remember that.".into(),
        });
        transcript
    }

    fn candidate(body: &str) -> ExtractionCandidate {
        ExtractionCandidate {
            memory_type: MemoryType::Semantic,
            body: body.into(),
            source_turn_ids: vec!["user-1".into()],
            evidence: vec![ProposalEvidence {
                turn_id: "user-1".into(),
                quote: body.into(),
            }],
            confidence: 0.8,
            origin: ProposalOrigin::UserStated,
            related_memory_ids: Vec::new(),
            relation: None,
        }
    }

    fn prepare(
        label: &str,
        candidate: ExtractionCandidate,
    ) -> (PathBuf, Vault, ExtractionQueue, MemoryStore, String) {
        let root = root(label);
        let vault = Vault::create(&root).unwrap();
        let mut queue = ExtractionQueue::open(&vault, "lyra").unwrap();
        let transcript = transcript();
        let job = queue
            .enqueue_transcript(&transcript, "assistant-1", 10)
            .unwrap();
        queue.claim_next().unwrap();
        let raw = serde_json::json!({"schema_version": 1, "proposals": [candidate]});
        let proposal = queue
            .accept_model_output(&job.id, &raw.to_string())
            .unwrap()
            .remove(0);
        queue
            .mark_proposal(&proposal.id, ProposalStatus::Accepted)
            .unwrap();
        let memories = MemoryStore::open(&vault, "lyra").unwrap();
        (root, vault, queue, memories, proposal.id)
    }

    #[test]
    fn duplicate_proposals_commit_one_memory_and_are_idempotent() {
        let (root, _vault, mut queue, mut memories, proposal_id) =
            prepare("duplicate", candidate("User likes tea."));
        let mut second_transcript = transcript();
        second_transcript.session_id = "session-2".into();
        let second_job = queue
            .enqueue_transcript(&second_transcript, "assistant-1", 10)
            .unwrap();
        queue.claim_next().unwrap();
        let second_raw =
            serde_json::json!({"schema_version": 1, "proposals": [candidate("User likes tea.")]});
        let second_proposal = queue
            .accept_model_output(&second_job.id, &second_raw.to_string())
            .unwrap()
            .remove(0);
        queue
            .mark_proposal(&second_proposal.id, ProposalStatus::Accepted)
            .unwrap();
        let mut service = ReconciliationService::new(&queue, &mut memories);
        let first = service.commit_accepted(&proposal_id).unwrap();
        let second = service.commit_accepted(&second_proposal.id).unwrap();
        let third = service.commit_accepted(&proposal_id).unwrap();
        assert!(matches!(first, CommitResult::Committed { .. }));
        assert!(matches!(second, CommitResult::Duplicate { .. }));
        assert!(matches!(third, CommitResult::Duplicate { .. }));
        assert_eq!(memories.list().unwrap().len(), 1);
        drop(memories);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deleted_memory_suppression_blocks_old_transcript_reconstruction() {
        let (root, _vault, queue, mut memories, proposal_id) =
            prepare("suppressed", candidate("User likes tea."));
        let mut deleted = MemoryRecord::new("old-memory", MemoryType::Semantic, "User likes tea.");
        deleted.source_session_id = Some("session-1".into());
        deleted.source_turn_ids = vec!["user-1".into()];
        memories.create(&deleted).unwrap();
        queue
            .suppress_memory(&deleted, "user deleted memory")
            .unwrap();
        let mut service = ReconciliationService::new(&queue, &mut memories);
        assert_eq!(
            service.commit_accepted(&proposal_id).unwrap(),
            CommitResult::Suppressed
        );
        assert_eq!(memories.list().unwrap().len(), 1);
        drop(memories);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn contradiction_links_are_persisted_and_markdown_repairs_the_index() {
        let mut contradiction = candidate("User now prefers busy cafes.");
        contradiction.related_memory_ids = vec!["existing".into()];
        contradiction.relation = Some(ProposalRelation::Contradicts);
        let (root, _vault, queue, mut memories, proposal_id) =
            prepare("contradiction", contradiction);
        let existing = MemoryRecord::new(
            "existing",
            MemoryType::Semantic,
            "User prefers quiet cafes.",
        );
        memories.create(&existing).unwrap();
        let mut service = ReconciliationService::new(&queue, &mut memories);
        let result = service.commit_accepted(&proposal_id).unwrap();
        let committed_id = match result {
            CommitResult::Committed { memory_id } => memory_id,
            other => panic!("unexpected result: {other:?}"),
        };
        let links = queue.relationships_for(&committed_id).unwrap();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].0, "existing");
        assert_eq!(links[0].1, "contradicts");
        memories.rebuild().unwrap();
        assert_eq!(memories.search("busy cafes", 10).unwrap().len(), 1);
        drop(memories);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn locked_related_memory_is_never_superseded() {
        let mut locked_candidate = candidate("New preference");
        locked_candidate.related_memory_ids = vec!["existing".into()];
        locked_candidate.relation = Some(ProposalRelation::Supersedes);
        let (root, _vault, queue, mut memories, proposal_id) = prepare("locked", locked_candidate);
        let mut existing = MemoryRecord::new("existing", MemoryType::Semantic, "Old preference");
        existing.locked = true;
        memories.create(&existing).unwrap();
        let mut service = ReconciliationService::new(&queue, &mut memories);
        assert_eq!(
            service.commit_accepted(&proposal_id).unwrap(),
            CommitResult::Locked {
                memory_id: "existing".into()
            }
        );
        drop(memories);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn newer_user_edit_survives_background_supersession() {
        let mut superseding = candidate("Background candidate");
        superseding.related_memory_ids = vec!["existing".into()];
        superseding.relation = Some(ProposalRelation::Supersedes);
        let (root, _vault, queue, mut memories, proposal_id) = prepare("newer-edit", superseding);
        let existing = MemoryRecord::new("existing", MemoryType::Semantic, "Newer user edit");
        memories.create(&existing).unwrap();
        let mut service = ReconciliationService::new(&queue, &mut memories);
        let result = service.commit_accepted(&proposal_id).unwrap();
        assert!(matches!(result, CommitResult::Committed { .. }));
        assert_eq!(
            memories
                .list()
                .unwrap()
                .iter()
                .find(|memory| memory.id == "existing")
                .unwrap()
                .body,
            "Newer user edit"
        );
        drop(memories);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }
}
