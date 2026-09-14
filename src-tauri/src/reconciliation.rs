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

    pub fn auto_commit(&mut self, proposal_id: &str) -> Result<CommitResult, ReconciliationError> {
        let proposal = self
            .queue
            .get_proposal(proposal_id)?
            .ok_or_else(|| ReconciliationError::InvalidInput("proposal not found".into()))?;
        let restore_on_failure = matches!(
            proposal.status,
            ProposalStatus::NeedsReview | ProposalStatus::Accepted
        );
        if proposal.status == ProposalStatus::NeedsReview {
            self.queue
                .mark_proposal(proposal_id, ProposalStatus::Accepted)?;
        }
        match self.commit_accepted(proposal_id) {
            Ok(CommitResult::Locked { memory_id }) => {
                self.queue
                    .mark_proposal(proposal_id, ProposalStatus::NeedsReview)?;
                Ok(CommitResult::Locked { memory_id })
            }
            Err(error) if restore_on_failure => {
                let _ = self
                    .queue
                    .mark_proposal(proposal_id, ProposalStatus::NeedsReview);
                Err(error)
            }
            other => other,
        }
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

    fn prepare_pending(
        label: &str,
        candidate: ExtractionCandidate,
    ) -> (
        PathBuf,
        Vault,
        ExtractionQueue,
        MemoryStore,
        String,
        TranscriptDocument,
    ) {
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
        let memories = MemoryStore::open(&vault, "lyra").unwrap();
        (root, vault, queue, memories, proposal.id, transcript)
    }

    fn prepare(
        label: &str,
        candidate: ExtractionCandidate,
    ) -> (PathBuf, Vault, ExtractionQueue, MemoryStore, String) {
        let (root, vault, queue, memories, proposal_id, _) = prepare_pending(label, candidate);
        queue
            .mark_proposal(&proposal_id, ProposalStatus::Accepted)
            .unwrap();
        (root, vault, queue, memories, proposal_id)
    }

    fn classify_and_maybe_auto_commit(
        queue: &ExtractionQueue,
        memories: &mut MemoryStore,
        proposal_id: &str,
        transcript: &TranscriptDocument,
    ) -> Result<Option<CommitResult>, ReconciliationError> {
        let proposal = queue
            .get_proposal(proposal_id)?
            .ok_or_else(|| ReconciliationError::InvalidInput("proposal not found".into()))?;
        let origin = crate::extraction::effective_origin(&proposal.candidate, transcript);
        if origin != proposal.candidate.origin {
            queue.reclassify_proposal(proposal_id, origin.clone())?;
        }
        if !crate::extraction::is_auto_writable(&origin) {
            return Ok(None);
        }
        let mut service = ReconciliationService::new(queue, memories);
        Ok(Some(service.auto_commit(proposal_id)?))
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

    #[test]
    fn auto_commits_user_stated_to_new_markdown() {
        let (root, vault, queue, mut memories, proposal_id, _) =
            prepare_pending("auto-user", candidate("User likes quiet cafes."));
        let mut service = ReconciliationService::new(&queue, &mut memories);
        let result = service.auto_commit(&proposal_id).unwrap();
        let memory_id = match result {
            CommitResult::Committed { memory_id } => memory_id,
            other => panic!("unexpected result: {other:?}"),
        };
        let path = vault
            .memory_path(&MemoryType::Semantic, &memory_id)
            .unwrap();
        assert!(path.exists());
        assert!(path
            .to_string_lossy()
            .replace('\\', "/")
            .contains("memories/semantic/"));
        assert!(queue
            .pending_proposals()
            .unwrap()
            .iter()
            .all(|proposal| proposal.id != proposal_id));
        let hits = memories.search("quiet cafes", 10).unwrap();
        assert!(hits
            .iter()
            .any(|hit| hit.chunk.contains("User likes quiet cafes.")));
        drop(memories);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn inferred_stays_in_inbox_and_is_not_a_file() {
        let mut inferred = candidate("User might prefer mornings.");
        inferred.origin = ProposalOrigin::Inferred;
        let (root, _vault, queue, mut memories, proposal_id, transcript) =
            prepare_pending("auto-inferred", inferred);
        let result =
            classify_and_maybe_auto_commit(&queue, &mut memories, &proposal_id, &transcript)
                .unwrap();
        assert!(result.is_none());
        assert_eq!(
            queue.get_proposal(&proposal_id).unwrap().unwrap().status,
            ProposalStatus::NeedsReview
        );
        assert!(!memories
            .list()
            .unwrap()
            .iter()
            .any(|memory| memory.body.contains("User might prefer mornings.")));
        drop(memories);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn assistant_labeled_user_stated_is_inferred() {
        let mut assistant_labeled = candidate("User likes tea.");
        assistant_labeled.source_turn_ids = vec!["assistant-1".into()];
        assistant_labeled.evidence[0].turn_id = "assistant-1".into();
        assistant_labeled.origin = ProposalOrigin::UserStated;
        let (root, _vault, queue, mut memories, proposal_id, transcript) =
            prepare_pending("auto-assistant", assistant_labeled);
        let result =
            classify_and_maybe_auto_commit(&queue, &mut memories, &proposal_id, &transcript)
                .unwrap();
        assert!(result.is_none());
        let proposal = queue.get_proposal(&proposal_id).unwrap().unwrap();
        assert_eq!(proposal.status, ProposalStatus::NeedsReview);
        assert_eq!(proposal.candidate.origin, ProposalOrigin::Inferred);
        assert!(!memories
            .list()
            .unwrap()
            .iter()
            .any(|memory| memory.body.contains("User likes tea.")));
        drop(memories);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fabricated_quote_is_not_auto_written_as_user_fact() {
        let mut fabricated = candidate("The user lives on Mars.");
        fabricated.evidence[0].quote = "I live on Mars.".into();
        fabricated.origin = ProposalOrigin::UserStated;
        let (root, _vault, queue, mut memories, proposal_id, transcript) =
            prepare_pending("fabricated-quote", fabricated);
        let result =
            classify_and_maybe_auto_commit(&queue, &mut memories, &proposal_id, &transcript)
                .unwrap();
        assert!(result.is_none());
        assert_eq!(
            queue
                .get_proposal(&proposal_id)
                .unwrap()
                .unwrap()
                .candidate
                .origin,
            ProposalOrigin::Inferred
        );
        assert!(memories.list().unwrap().is_empty());
        drop(memories);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn duplicate_does_not_write_second_file() {
        let (root, _vault, mut queue, mut memories, proposal_id, _) =
            prepare_pending("auto-duplicate", candidate("User likes tea."));
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
        let mut service = ReconciliationService::new(&queue, &mut memories);
        let first = service.auto_commit(&proposal_id).unwrap();
        let second = service.auto_commit(&second_proposal.id).unwrap();
        assert!(matches!(first, CommitResult::Committed { .. }));
        assert!(matches!(second, CommitResult::Duplicate { .. }));
        assert_eq!(memories.list().unwrap().len(), 1);
        drop(memories);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn supersession_creates_new_file_and_records_link() {
        let mut superseding = candidate("Updated preference");
        superseding.related_memory_ids = vec!["existing".into()];
        superseding.relation = Some(ProposalRelation::Supersedes);
        let (root, vault, queue, mut memories, proposal_id, _) =
            prepare_pending("auto-supersede", superseding);
        let existing = MemoryRecord::new("existing", MemoryType::Semantic, "Older preference");
        memories.create(&existing).unwrap();
        let old_path = vault
            .memory_path(&MemoryType::Semantic, "existing")
            .unwrap();
        let old_bytes = std::fs::read(&old_path).unwrap();
        let mut service = ReconciliationService::new(&queue, &mut memories);
        let result = service.auto_commit(&proposal_id).unwrap();
        let new_id = match result {
            CommitResult::Committed { memory_id } => memory_id,
            other => panic!("unexpected result: {other:?}"),
        };
        assert_ne!(new_id, "existing");
        let links = queue.relationships_for(&new_id).unwrap();
        assert!(links
            .iter()
            .any(|(to, relation, _)| { to == "existing" && relation == "supersedes" }));
        assert_eq!(std::fs::read(&old_path).unwrap(), old_bytes);
        assert_eq!(
            memories
                .list()
                .unwrap()
                .iter()
                .find(|memory| memory.id == "existing")
                .unwrap()
                .body,
            "Older preference"
        );
        drop(memories);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn locked_related_memory_goes_to_inbox() {
        let mut locked_candidate = candidate("New preference");
        locked_candidate.related_memory_ids = vec!["existing".into()];
        locked_candidate.relation = Some(ProposalRelation::Supersedes);
        let (root, vault, queue, mut memories, proposal_id, _) =
            prepare_pending("auto-locked", locked_candidate);
        let mut existing = MemoryRecord::new("existing", MemoryType::Semantic, "Old preference");
        existing.locked = true;
        memories.create(&existing).unwrap();
        let locked_path = vault
            .memory_path(&MemoryType::Semantic, "existing")
            .unwrap();
        let locked_bytes = std::fs::read(&locked_path).unwrap();
        let mut service = ReconciliationService::new(&queue, &mut memories);
        assert_eq!(
            service.auto_commit(&proposal_id).unwrap(),
            CommitResult::Locked {
                memory_id: "existing".into()
            }
        );
        assert_eq!(
            queue.get_proposal(&proposal_id).unwrap().unwrap().status,
            ProposalStatus::NeedsReview
        );
        assert_eq!(std::fs::read(&locked_path).unwrap(), locked_bytes);
        drop(memories);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deleted_memory_is_not_resurrected() {
        let (root, _vault, queue, mut memories, proposal_id, _) =
            prepare_pending("auto-deleted", candidate("User likes tea."));
        let mut deleted = MemoryRecord::new("old-memory", MemoryType::Semantic, "User likes tea.");
        deleted.source_session_id = Some("session-1".into());
        deleted.source_turn_ids = vec!["user-1".into()];
        queue
            .suppress_memory(&deleted, "user deleted memory")
            .unwrap();
        let mut service = ReconciliationService::new(&queue, &mut memories);
        assert_eq!(
            service.auto_commit(&proposal_id).unwrap(),
            CommitResult::Suppressed
        );
        assert!(memories.list().unwrap().is_empty());
        drop(memories);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }

    fn sabotage_semantic_writes(vault: &Vault) {
        let semantic = vault.root().join("memories").join("semantic");
        if semantic.is_dir() {
            std::fs::remove_dir_all(&semantic).unwrap();
        }
        if let Some(parent) = semantic.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&semantic, b"not-a-directory").unwrap();
    }

    #[test]
    fn auto_commit_error_restores_needs_review() {
        let (root, vault, queue, mut memories, proposal_id, _) =
            prepare_pending("auto-commit-err", candidate("User likes tea."));
        sabotage_semantic_writes(&vault);
        let mut service = ReconciliationService::new(&queue, &mut memories);
        assert!(service.auto_commit(&proposal_id).is_err());
        assert_eq!(
            queue.get_proposal(&proposal_id).unwrap().unwrap().status,
            ProposalStatus::NeedsReview
        );
        assert!(queue
            .pending_proposals()
            .unwrap()
            .iter()
            .any(|proposal| proposal.id == proposal_id));
        drop(memories);
        drop(queue);
        std::fs::remove_dir_all(root).unwrap();
    }
}
