import { invoke } from "@tauri-apps/api/core";
import type {
  CharacterDefinition,
  RetrievedMemory,
  TranscriptDocument,
  TranscriptTurn,
} from "./conversation";
import type { ProviderConfig } from "./providers";

export type InitiativeSettings = {
  schema_version: number;
  enabled: boolean;
  notifications_enabled: boolean;
  min_inactive_seconds: number;
  cooldown_seconds: number;
  max_per_day: number;
  max_ignored: number;
  quiet_start_minute: number;
  quiet_end_minute: number;
};

export type InitiativeState = {
  schema_version: number;
  last_user_activity_at: number | null;
  last_initiative_at: number | null;
  day_key: number;
  sent_today: number;
  ignored_streak: number;
  unanswered: boolean;
  resume_suppressed_until: number;
};

export type InitiativeSnapshot = {
  settings: InitiativeSettings;
  state: InitiativeState;
  decision: {
    eligible: boolean;
    reasons: string[];
    effective_cooldown_seconds: number;
  };
};

export type InitiativeRequest = {
  character: CharacterDefinition;
  provider: ProviderConfig;
  session_id: string;
  request_id: string;
  topic_context: string;
  generation: number;
  started_at: number;
  application_prompt?: string;
};

export type InitiativeOutcome = {
  transcript: TranscriptDocument;
  initiative_turn: TranscriptTurn | null;
  assistant_turn: TranscriptTurn | null;
  retrieved_memories: RetrievedMemory[];
  choice: "Silence" | "Message";
  status: "Delivered" | "Silenced" | "Cancelled" | "Failed";
};

export const initiativeClient = {
  snapshot(vaultRoot: string, characterId: string, now: number) {
    return invoke<InitiativeSnapshot>("initiative_snapshot", {
      vaultRoot,
      characterId,
      now,
      modelAvailable: true,
      resourceAvailable: true,
      utcOffsetMinutes: -new Date().getTimezoneOffset(),
    });
  },
 saveSettings(vaultRoot: string, characterId: string, settings: InitiativeSettings) {
   return invoke<void>("initiative_save_settings", { vaultRoot, characterId, settings });
 },
  startScheduler(vaultRoot: string, characterId: string, sessionId: string) {
    return invoke<void>("initiative_scheduler_start", {
      vaultRoot,
      characterId,
      sessionId,
      utcOffsetMinutes: -new Date().getTimezoneOffset(),
    });
  },
  stopScheduler(characterId: string, sessionId: string) {
    return invoke<boolean>("initiative_scheduler_stop", { characterId, sessionId });
  },
  recordResume(vaultRoot: string, characterId: string, now: number) {
    return invoke<InitiativeState>("initiative_record_resume", { vaultRoot, characterId, now });
  },
  send(
    vaultRoot: string,
    request: InitiativeRequest,
    currentGeneration: number,
    latestUserActivityAt: number | null,
  ) {
    return invoke<InitiativeOutcome>("initiative_send", {
      vaultRoot,
      request,
      currentGeneration,
      latestUserActivityAt,
    });
  },
};
