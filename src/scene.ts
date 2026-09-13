import { invoke } from "@tauri-apps/api/core";

export type PersonaNotes = {
  schema_version: number;
  body: string;
};

export type SceneSettings = {
  schema_version: number;
  default_local: string | null;
};

export type LocalRecord = {
  schema_version: number;
  id: string;
  title: string;
  body: string;
};

export type LocalSummary = {
  id: string;
  title: string;
};

export type SessionLocalSelection =
  | { kind: "default" }
  | { kind: "none" }
  | { kind: "local"; id: string };

export function selectionFromTranscript(local: string | null | undefined): SessionLocalSelection {
  if (local == null) return { kind: "default" };
  if (local.trim() === "") return { kind: "none" };
  return { kind: "local", id: local };
}

export function localIdFromTitle(title: string) {
  const slug = title
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 128);
  return slug;
}

export const sceneClient = {
  loadPersona(vaultRoot: string) {
    return invoke<PersonaNotes>("persona_load", { vaultRoot });
  },
  savePersona(vaultRoot: string, notes: PersonaNotes) {
    return invoke<PersonaNotes>("persona_save", { vaultRoot, notes });
  },
  loadSettings(vaultRoot: string) {
    return invoke<SceneSettings>("scene_settings_load", { vaultRoot });
  },
  saveSettings(vaultRoot: string, settings: SceneSettings) {
    return invoke<SceneSettings>("scene_settings_save", { vaultRoot, settings });
  },
  listLocals(vaultRoot: string) {
    return invoke<LocalSummary[]>("locals_list", { vaultRoot });
  },
  loadLocal(vaultRoot: string, id: string) {
    return invoke<LocalRecord>("local_load", { vaultRoot, id });
  },
  saveLocal(vaultRoot: string, record: LocalRecord) {
    return invoke<LocalRecord>("local_save", { vaultRoot, record });
  },
  deleteLocal(vaultRoot: string, id: string) {
    return invoke<void>("local_delete", { vaultRoot, id });
  },
};
