import type { CharacterGeneration } from "./generation";

export type CharacterEditorSection = "identity" | "model" | "persona" | "scenes";

export function parseGenerationFields(
  chatModel: string | null,
  temperatureText: string,
  maxTokensText: string,
): CharacterGeneration {
  let temperature: number | null = null;
  if (temperatureText.trim()) {
    const parsed = Number(temperatureText);
    if (!Number.isFinite(parsed) || parsed < 0 || parsed > 2) {
      throw new Error("Temperature must be between 0 and 2, or empty to inherit.");
    }
    temperature = parsed;
  }

  let maxTokens: number | null = null;
  if (maxTokensText.trim()) {
    const parsed = Number(maxTokensText);
    if (!Number.isInteger(parsed) || parsed < 16 || parsed > 2048) {
      throw new Error("Max tokens must be a whole number from 16 to 2048, or empty to inherit.");
    }
    maxTokens = parsed;
  }

  return {
    schema_version: 1,
    chat_model: chatModel?.trim() || null,
    temperature,
    max_tokens: maxTokens,
  };
}
