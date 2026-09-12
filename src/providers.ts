import { invoke } from "@tauri-apps/api/core";

export type ProviderKind = "ollama" | "open_ai_compatible";

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
};

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
