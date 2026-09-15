import { invoke } from "@tauri-apps/api/core";

export type ProviderKind = "ollama" | "lm_studio" | "open_ai_compatible";

export type ProviderCapabilities = {
  health: boolean;
  model_discovery: boolean;
  chat_streaming: boolean;
  cancellation: boolean;
  embeddings: boolean;
};

export type ProviderConfig = {
  id: string;
  kind: ProviderKind;
  endpoint: string;
  chat_model: string;
  embedding_model?: string | null;
  bearer_token?: string | null;
  has_bearer_token: boolean;
};

export function providerDefaults(kind: ProviderKind): Pick<ProviderConfig, "id" | "kind" | "endpoint"> {
  switch (kind) {
    case "ollama":
      return { id: "ollama-local", kind, endpoint: "http://127.0.0.1:11434" };
    case "lm_studio":
      return { id: "lm-studio-local", kind, endpoint: "http://127.0.0.1:1234/v1" };
    case "open_ai_compatible":
      return { id: "openai-compatible-local", kind, endpoint: "http://127.0.0.1:8080/v1" };
  }
}

export type RedactedProviderConfig = Omit<ProviderConfig, "bearer_token"> & {
  has_bearer_token: boolean;
};

export type ProviderSettings = {
  schema_version: number;
  active_provider_id: string | null;
  providers: ProviderConfig[];
};

export type HealthResponse = {
  provider_id: string;
  reachable: boolean;
  detail: string | null;
};

export type ModelDescriptor = {
  id: string;
  kind: string;
  supports_chat: boolean;
  supports_embeddings: boolean;
};

export type DiscoveryResponse = {
  provider_id: string;
  models: ModelDescriptor[];
};

export type EmbeddingRequest = {
  provider_id: string;
  model: string;
  input: string[];
};

export type EmbeddingResponse = {
  model: string;
  dimensions: number;
  vectors: number[][];
};

export const providerClient = {
  capabilities(kind: ProviderKind) {
    return invoke<ProviderCapabilities>("provider_capabilities", { kind });
  },
  validate(config: ProviderConfig) {
    return invoke<RedactedProviderConfig>("validate_provider_config", { config });
  },
  loadSettings() {
    return invoke<ProviderSettings>("load_provider_settings");
  },
  saveSettings(settings: ProviderSettings) {
    return invoke<void>("save_provider_settings", { settings });
  },
  health(config: ProviderConfig) {
    return invoke<HealthResponse>("provider_health", { config });
  },
  discover(config: ProviderConfig) {
    return invoke<DiscoveryResponse>("provider_discover", { config });
  },
  embed(config: ProviderConfig, request: EmbeddingRequest) {
    return invoke<EmbeddingResponse>("provider_embed", { config, request });
  },
};
