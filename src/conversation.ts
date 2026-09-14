import { Channel, invoke } from "@tauri-apps/api/core";
import type { ProviderConfig } from "./providers";
import type { SessionLocalSelection } from "./scene";

export type CharacterDefinition = {
  schema_version: number;
  id: string;
  name: string;
  summary: string;
  system_prompt: string;
  traits: string[];
  boundaries: string[];
  tags: string[];
};

export type RequestSnapshot = {
  character: CharacterDefinition;
  provider: ProviderConfig;
  session_id: string;
  user_turn_id: string;
  user_content: string;
  use_hybrid_retrieval: boolean;
  application_prompt?: string;
  expected_transcript_fingerprint?: string | null;
};

export type TurnRole = "system" | "user" | "assistant" | "initiative";
export type TurnStatus = "complete" | "interrupted" | "failed" | "superseded";

export type TranscriptTurn = {
  id: string;
  timestamp: string;
  role: TurnRole;
  status: TurnStatus;
  content: string;
};

export type TranscriptDocument = {
  schema_version: number;
  session_id: string;
  character_id: string;
  created_at: string;
  updated_at: string;
  local?: string | null;
  title?: string;
  archived?: boolean;
  turns: TranscriptTurn[];
};

export type ConversationOutcome = {
  transcript: TranscriptDocument;
  assistant_turn: TranscriptTurn;
  retrieved_memories: RetrievedMemory[];
  context_inspection: ContextInspection;
  transcript_fingerprint: string | null;
};

export type ContextInspection = {
  retrieval_mode: string;
  fallback_reason: string | null;
  estimated_input_tokens: number;
  input_token_limit: number;
  reserved_output_tokens: number;
  omitted_turns: number;
  selected_memory_tokens: number;
  candidate_memories: number;
  omitted_memories: number;
};

export type ChatStreamEvent =
  | { event: "started"; data: { cancellation_id: string } }
  | { event: "delta"; data: { text: string } }
  | { event: "completed"; data: { finish_reason: string | null } }
  | { event: "cancelled" }
  | { event: "failed"; data: { message: string } };

export type ConversationResume = {
  character: CharacterDefinition;
  transcript: TranscriptDocument | null;
  transcript_fingerprint: string | null;
};

export type SessionSummary = {
  session_id: string;
  title: string;
  created_at: string;
  updated_at: string;
  turn_count: number;
  preview: string;
  archived: boolean;
  snippet?: string | null;
};

export type RetrievedMemory = {
  memory_id: string;
  source_path: string;
  content: string;
  salience: number;
  pinned: boolean;
  estimated_tokens: number;
  reasons: string[];
  lexical_score: number | null;
  semantic_score: number | null;
};

export const conversationClient = {
  resume(vaultRoot: string, expectedCharacterId?: string) {
    return invoke<ConversationResume>("conversation_resume", {
      vaultRoot,
      expectedCharacterId: expectedCharacterId ?? null,
    });
  },
  listSessions(vaultRoot: string, includeArchived = false) {
    return invoke<SessionSummary[]>("conversation_list_sessions", {
      vaultRoot,
      includeArchived,
    });
  },
  searchSessions(vaultRoot: string, query: string) {
    return invoke<SessionSummary[]>("conversation_search_sessions", { vaultRoot, query });
  },
  renameSession(vaultRoot: string, sessionId: string, title: string) {
    return invoke<TranscriptDocument>("conversation_rename_session", {
      vaultRoot,
      sessionId,
      title,
    });
  },
  setSessionArchived(vaultRoot: string, sessionId: string, archived: boolean) {
    return invoke<TranscriptDocument>("conversation_set_session_archived", {
      vaultRoot,
      sessionId,
      archived,
    });
  },
  openSession(vaultRoot: string, sessionId: string) {
    return invoke<ConversationResume>("conversation_open_session", { vaultRoot, sessionId });
  },
  startSession(vaultRoot: string) {
    return invoke<ConversationResume>("conversation_start_session", { vaultRoot });
  },
  setSessionLocal(
    vaultRoot: string,
    sessionId: string,
    selection: SessionLocalSelection,
  ) {
    return invoke<TranscriptDocument>("conversation_set_session_local", {
      vaultRoot,
      sessionId,
      selection,
    });
  },
  send(vaultRoot: string, snapshot: RequestSnapshot, onMessage: (event: ChatStreamEvent) => void) {
    const onEvent = new Channel<ChatStreamEvent>();
    onEvent.onmessage = onMessage;
    return invoke<ConversationOutcome>("conversation_send", {
      vaultRoot,
      snapshot,
      onEvent,
    });
  },
  retry(vaultRoot: string, snapshot: RequestSnapshot, onMessage: (event: ChatStreamEvent) => void) {
    const onEvent = new Channel<ChatStreamEvent>();
    onEvent.onmessage = onMessage;
    return invoke<ConversationOutcome>("conversation_retry", {
      vaultRoot,
      snapshot,
      onEvent,
    });
  },
  regenerate(vaultRoot: string, snapshot: RequestSnapshot, onMessage: (event: ChatStreamEvent) => void) {
    const onEvent = new Channel<ChatStreamEvent>();
    onEvent.onmessage = onMessage;
    return invoke<ConversationOutcome>("conversation_regenerate", {
      vaultRoot,
      snapshot,
      onEvent,
    });
  },
  continueReply(vaultRoot: string, snapshot: RequestSnapshot, onMessage: (event: ChatStreamEvent) => void) {
    const onEvent = new Channel<ChatStreamEvent>();
    onEvent.onmessage = onMessage;
    return invoke<ConversationOutcome>("conversation_continue", {
      vaultRoot,
      snapshot,
      onEvent,
    });
  },
  editLastUser(vaultRoot: string, snapshot: RequestSnapshot, onMessage: (event: ChatStreamEvent) => void) {
    const onEvent = new Channel<ChatStreamEvent>();
    onEvent.onmessage = onMessage;
    return invoke<ConversationOutcome>("conversation_edit_last_user", {
      vaultRoot,
      snapshot,
      onEvent,
    });
  },
  cancel(characterId: string, sessionId: string) {
    return invoke<boolean>("conversation_cancel", { characterId, sessionId });
  },
};
