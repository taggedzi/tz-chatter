import { invoke } from "@tauri-apps/api/core";

export type MemoryType = "people" | "episodic" | "semantic" | "relationships" | "open_threads";
export type MemoryReviewStatus = "accepted" | "needs_review" | "excluded";

export type MemoryRecord = {
  schema_version: number;
  id: string;
  memory_type: MemoryType;
  created_at: string;
  updated_at: string;
  source_session_id: string | null;
  source_turn_ids: string[];
  topics: string[];
  salience: number;
  confidence: number;
  review_status: MemoryReviewStatus;
  pinned: boolean;
  locked: boolean;
  body: string;
};

export type MemoryCommitResult =
  | { committed: { memory_id: string } }
  | { duplicate: { memory_id: string } }
  | "suppressed"
  | { locked: { memory_id: string } };

export type MemoryProposal = {
  id: string;
  job_id: string;
  character_id: string;
  source_session_id: string;
  candidate: {
    memory_type: MemoryType;
    body: string;
    source_turn_ids: string[];
    evidence: { turn_id: string; quote: string }[];
    confidence: number;
    origin: "user_stated" | "character_fact" | "conversation_event" | "inferred";
    related_memory_ids: string[];
    relation: "contradicts" | "supersedes" | "related" | null;
  };
  status: "needs_review" | "accepted" | "rejected" | "superseded" | "committed";
};

export type MemoryConflictRelation = "contradicts" | "supersedes";

export type MemoryConflictSide = {
  id: string;
  memory_type: MemoryType;
  review_status: MemoryReviewStatus;
  locked: boolean;
  updated_at: string;
  excerpt: string;
};

export type MemoryConflictPair = {
  relation: MemoryConflictRelation;
  from: MemoryConflictSide;
  to: MemoryConflictSide;
  proposal_id: string;
  created_at: string;
};

export const memoryClient = {
  reviewQueue(vaultRoot: string, characterId: string) {
    return invoke<MemoryProposal[]>("memory_review_queue", { vaultRoot, characterId });
  },
  conflictPairs(vaultRoot: string, characterId: string) {
    return invoke<MemoryConflictPair[]>("memory_conflict_pairs", { vaultRoot, characterId });
  },
  excludeConflictSide(
    vaultRoot: string,
    characterId: string,
    pair: Pick<MemoryConflictPair, "from" | "to" | "relation">,
    targetId: string,
  ) {
    return invoke<void>("memory_exclude_conflict_side", {
      vaultRoot,
      characterId,
      fromId: pair.from.id,
      toId: pair.to.id,
      relation: pair.relation,
      targetId,
    });
  },
  editProposal(vaultRoot: string, characterId: string, proposalId: string, body: string, confidence: number) {
    return invoke<MemoryProposal>("memory_edit_proposal", { vaultRoot, characterId, proposalId, body, confidence });
  },
  browse(vaultRoot: string, characterId: string) {
    return invoke<MemoryRecord[]>("memory_browse", { vaultRoot, characterId });
  },
  search(vaultRoot: string, characterId: string, query: string, limit = 20) {
    return invoke<MemoryRecord[]>("memory_search", {
      vaultRoot,
      characterId,
      query,
      limit,
    });
  },
  sourcePath(vaultRoot: string, characterId: string, sourcePath: string) {
    return invoke<string>("memory_source_path", { vaultRoot, characterId, sourcePath });
  },
  upsert(
    vaultRoot: string,
    characterId: string,
    memory: MemoryRecord,
    original?: Pick<MemoryRecord, "memory_type" | "id">,
  ) {
    return invoke<void>("memory_upsert", {
      vaultRoot,
      characterId,
      memoryRecord: memory,
      originalMemoryType: original?.memory_type ?? null,
      originalId: original?.id ?? null,
    });
  },
  remove(vaultRoot: string, characterId: string, memoryType: MemoryType, id: string) {
    return invoke<void>("memory_delete", {
      vaultRoot,
      characterId,
      memoryType,
      id,
    });
  },
  acceptProposal(vaultRoot: string, characterId: string, proposalId: string) {
    return invoke<void>("memory_accept_proposal", { vaultRoot, characterId, proposalId });
  },
  rejectProposal(vaultRoot: string, characterId: string, proposalId: string) {
    return invoke<void>("memory_reject_proposal", { vaultRoot, characterId, proposalId });
  },
  commitProposal(vaultRoot: string, characterId: string, proposalId: string) {
    return invoke<MemoryCommitResult>("memory_commit_proposal", { vaultRoot, characterId, proposalId });
  },
  repairIndex(vaultRoot: string, characterId: string) {
    return invoke<void>("memory_repair_index", { vaultRoot, characterId });
  },
};
