import { invoke } from "@tauri-apps/api/core";

export type CharacterGeneration = {
  schema_version: number;
  chat_model: string | null;
  temperature: number | null;
  max_tokens: number | null;
};

export const emptyGeneration = (): CharacterGeneration => ({
  schema_version: 1,
  chat_model: null,
  temperature: null,
  max_tokens: null,
});

export const generationClient = {
  load(vaultRoot: string) {
    return invoke<CharacterGeneration>("generation_load", { vaultRoot });
  },
  save(vaultRoot: string, settings: CharacterGeneration) {
    return invoke<CharacterGeneration>("generation_save", { vaultRoot, settings });
  },
};
