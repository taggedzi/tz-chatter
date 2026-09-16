//! Opt-in real local-provider evidence; excluded from ordinary deterministic checks.
use crate::{
    characters,
    connections::ProviderClient,
    conversation::{ConversationService, RequestSnapshot},
    featured,
    generation::{self, CharacterGeneration},
    memory::MemoryStore,
    providers::{ProviderConfig, ProviderKind},
    scene,
    storage::{TurnStatus, Vault},
};
use std::{fs, path::PathBuf, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
#[ignore = "requires an already-installed local Ollama model; performs real chat and extraction"]
async fn holmes_live_chat_extraction_and_new_session_recall() {
    let scratch =
        Scratch(std::env::temp_dir().join(format!("tz-holmes-live-{}", uuid::Uuid::new_v4())));
    let item = featured::install_holmes(
        &scratch.0.join("library.json"),
        &scratch.0.join("characters"),
    )
    .unwrap();
    let vault = Vault::open(&item.vault_root).unwrap();
    let character = vault.load_character().unwrap();
    let provider = ProviderConfig {
        id: "ollama-holmes-test".into(),
        kind: ProviderKind::Ollama,
        endpoint: "http://127.0.0.1:11434".into(),
        chat_model: std::env::var("HOLMES_TEST_MODEL").unwrap_or_else(|_| "qwen2.5:7b".into()),
        embedding_model: None,
        bearer_token: None,
    };
    let client = Arc::new(ProviderClient::default());
    let discovery = client.discover(&provider).await.unwrap();
    assert!(
        discovery
            .models
            .iter()
            .any(|model| model.id == provider.chat_model),
        "model must already be installed"
    );
    generation::save(
        &vault,
        &CharacterGeneration {
            temperature: Some(0.4),
            max_tokens: Some(384),
            ..Default::default()
        },
    )
    .unwrap();
    let service = ConversationService::new(client.clone());
    let snapshot = |session: &str, text: &str| RequestSnapshot {
        character: character.clone(),
        provider: provider.clone(),
        session_id: session.into(),
        user_turn_id: uuid::Uuid::new_v4().to_string(),
        user_content: text.into(),
        use_hybrid_retrieval: false,
        application_prompt: characters::DEFAULT_APPLICATION_PROMPT.into(),
        expected_transcript_fingerprint: None,
    };
    let lore = tokio::time::timeout(
        Duration::from_secs(180),
        service.send(
            &vault,
            snapshot("lore", "What does Norbury mean to you?"),
            CancellationToken::new(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    println!("LORE {}", serde_json::to_string(&lore).unwrap());
    assert_eq!(lore.assistant_turn.status, TurnStatus::Complete);
    assert!(lore
        .retrieved_memories
        .iter()
        .any(|note| note.memory_id == "holmes-norbury"));
    let token = format!("copper-{}", &uuid::Uuid::new_v4().simple().to_string()[..8]);
    let text = format!("In this fictional case, I am looking for a silver compass. Our case token is {token}. Please remember the compass case token for our next conversation.");
    let first = tokio::time::timeout(
        Duration::from_secs(180),
        service.send(
            &vault,
            snapshot("first-visit", &text),
            CancellationToken::new(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    println!("FIRST {}", serde_json::to_string(&first).unwrap());
    assert_eq!(first.assistant_turn.status, TurnStatus::Complete);
    tokio::time::timeout(
        Duration::from_secs(240),
        crate::process_extraction_queue(
            item.vault_root.clone(),
            provider.clone(),
            character.clone(),
            CancellationToken::new(),
            client,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let mut store = MemoryStore::open(&vault, &character.id).unwrap();
    let records = store.list().unwrap();
    println!(
        "FORMED {}",
        serde_json::to_string(
            &records
                .iter()
                .filter(|note| note.source_session_id.is_some())
                .collect::<Vec<_>>()
        )
        .unwrap()
    );
    assert!(
        records.iter().any(|note| note.body.contains(&token)
            && note.source_session_id.as_deref() == Some("first-visit")),
        "live worker did not persist the novel token"
    );
    drop(store);
    // Reopen disk state, construct a new service, and use a new empty transcript.
    let reopened = Vault::open(&item.vault_root).unwrap();
    let resumed_service = ConversationService::with_provider_client();
    let recall = tokio::time::timeout(
        Duration::from_secs(180),
        resumed_service.send(
            &reopened,
            snapshot(
                "return-visit",
                "What was the case token for my silver compass?",
            ),
            CancellationToken::new(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    println!("RECALL {}", serde_json::to_string(&recall).unwrap());
    assert_eq!(recall.assistant_turn.status, TurnStatus::Complete);
    assert!(recall
        .retrieved_memories
        .iter()
        .any(|note| note.content.contains(&token)));
    assert!(recall
        .assistant_turn
        .content
        .to_lowercase()
        .contains(&token.to_lowercase()));
    assert_eq!(
        recall.transcript.turns.len(),
        2,
        "recall must not reuse the first session history"
    );
    let mut letter = crate::storage::TranscriptDocument::new("letter", &character.id);
    scene::apply_selection(
        &mut letter,
        &scene::SessionLocalSelection::Local {
            id: "correspondence".into(),
        },
    );
    reopened.save_transcript(&letter).unwrap();
    let correspondence = tokio::time::timeout(
        Duration::from_secs(180),
        resumed_service.send(
            &reopened,
            snapshot(
                "letter",
                "What can you observe about my clothes from this correspondence?",
            ),
            CancellationToken::new(),
        ),
    )
    .await
    .unwrap()
    .unwrap();
    println!(
        "CORRESPONDENCE {}",
        serde_json::to_string(&correspondence).unwrap()
    );
    assert_eq!(correspondence.assistant_turn.status, TurnStatus::Complete);
    println!(
        "HOLMES LIVE PASS model={} token={}",
        provider.chat_model, token
    );
}
