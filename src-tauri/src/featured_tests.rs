use crate::{
    characters::{self, DEFAULT_APPLICATION_PROMPT},
    featured::install_holmes,
    memory::MemoryStore,
    portability::{export_pack, import_pack, PackManifest},
    prompt::{build_prompt_with_memories, PromptBudget, PromptLayers},
    retrieval::{retrieve, RetrievalBudget},
    scene,
    storage::{MemoryRecord, MemoryReviewStatus, MemoryType, TranscriptDocument, Vault},
};
use std::{fs, path::PathBuf};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("tz-holmes-{}", uuid::Uuid::new_v4())))
    }
    fn install(&self) -> Vault {
        let item = install_holmes(
            &self.0.join("config/library.json"),
            &self.0.join("characters"),
        )
        .unwrap();
        Vault::open(item.vault_root).unwrap()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn holmes_embedded_pack_is_complete_portable_and_has_no_user_history() {
    let scratch = Scratch::new();
    let vault = scratch.install();
    let source =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../examples/characters/sherlock-holmes");
    let manifest: PackManifest =
        serde_json::from_slice(&fs::read(source.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest.files.len(), 23);
    for relative in &manifest.files {
        assert_eq!(
            fs::read(source.join(relative)).unwrap(),
            fs::read(vault.root().join(relative)).unwrap(),
            "{relative}"
        );
    }
    let character = vault.load_character().unwrap();
    assert_eq!(character.id, "sherlock-holmes");
    assert!(vault.list_transcripts().unwrap().is_empty());
    assert_eq!(scene::list_locals(&vault).unwrap().len(), 3);
    assert_eq!(
        scene::load_scene_settings(&vault)
            .unwrap()
            .default_local
            .as_deref(),
        Some("baker-street")
    );
    assert!(characters::load_portrait(vault.root()).unwrap().is_some());
    let mut store = MemoryStore::open(&vault, &character.id).unwrap();
    let memories = store.list().unwrap();
    assert_eq!(memories.len(), 16);
    for note in memories {
        assert!(note.source_session_id.is_none() && note.source_turn_ids.is_empty());
        assert!(note.locked && !note.pinned);
        assert_eq!(note.review_status, MemoryReviewStatus::Accepted);
        assert_ne!(note.memory_type, MemoryType::OpenThreads);
        assert!(
            note.body.contains("https://"),
            "missing portable source in {}",
            note.id
        );
    }
    drop(store);
    let exported = scratch.0.join("exported");
    let round_trip_manifest = export_pack(&vault, &exported).unwrap();
    assert_eq!(round_trip_manifest.files.len(), manifest.files.len());
    let imported = scratch.0.join("imported");
    import_pack(&exported, &imported, false).unwrap();
    for relative in manifest.files {
        assert_eq!(
            fs::read(vault.root().join(&relative)).unwrap(),
            fs::read(imported.join(&relative)).unwrap(),
            "round trip: {relative}"
        );
    }
}

#[test]
fn holmes_installer_refuses_existing_vault_and_unrelated_directory() {
    let scratch = Scratch::new();
    let vault = scratch.install();
    let mut changed = vault.load_character().unwrap();
    changed.system_prompt = "My edited Holmes identity.".into();
    vault.save_character(&changed).unwrap();
    let library = scratch.0.join("config/library.json");
    let before = fs::read(&library).unwrap();
    assert!(install_holmes(&library, &scratch.0.join("characters")).is_err());
    assert_eq!(vault.load_character().unwrap(), changed);
    assert_eq!(fs::read(&library).unwrap(), before);
    assert_eq!(characters::list_library(&library).unwrap().entries.len(), 1);
    let unrelated_parent = scratch.0.join("unrelated");
    fs::create_dir_all(unrelated_parent.join("sherlock-holmes")).unwrap();
    fs::write(
        unrelated_parent.join("sherlock-holmes/keep.txt"),
        "user data",
    )
    .unwrap();
    assert!(install_holmes(&library, &unrelated_parent).is_err());
    assert_eq!(
        fs::read_to_string(unrelated_parent.join("sherlock-holmes/keep.txt")).unwrap(),
        "user data"
    );
    assert_eq!(
        fs::read_dir(scratch.0.join("characters")).unwrap().count(),
        1
    );
}

#[test]
fn holmes_library_failure_preserves_installed_copy_for_recovery() {
    let scratch = Scratch::new();
    fs::create_dir_all(&scratch.0).unwrap();
    let library = scratch.0.join("invalid-library.json");
    fs::write(&library, "invalid JSON").unwrap();
    let error = install_holmes(&library, &scratch.0.join("characters")).unwrap_err();
    assert!(error.to_string().contains("Use Characters > Add"));
    assert_eq!(fs::read_to_string(&library).unwrap(), "invalid JSON");
    let vault = Vault::open(scratch.0.join("characters/sherlock-holmes")).unwrap();
    assert_eq!(vault.load_character().unwrap().id, "sherlock-holmes");
    assert_eq!(
        fs::read_dir(scratch.0.join("characters")).unwrap().count(),
        1
    );
}

#[test]
fn holmes_retrieval_has_source_provenance_and_fits_default_prompt_in_every_scene() {
    let scratch = Scratch::new();
    let vault = scratch.install();
    let character = vault.load_character().unwrap();
    let mut store = MemoryStore::open(&vault, &character.id).unwrap();
    for (query, expected) in [
        ("What does Norbury mean to you?", "holmes-norbury"),
        ("What is in your Persian slipper?", "holmes-rooms"),
        (
            "Tell me about Mycroft and the Diogenes Club.",
            "holmes-mycroft",
        ),
        (
            "How many steps lead up to your room?",
            "holmes-seventeen-steps",
        ),
        (
            "Tell me about the dog in Silver Blaze.",
            "holmes-silver-blaze",
        ),
    ] {
        let selected = retrieve(&mut store, query, RetrievalBudget::default(), &[]).unwrap();
        let note = selected
            .iter()
            .find(|note| note.memory_id == expected)
            .unwrap_or_else(|| panic!("missing {expected}: {selected:?}"));
        assert!(
            note.content.contains("https://"),
            "retrieved chunk lost citation: {note:?}"
        );
        for local in scene::list_locals(&vault).unwrap() {
            let mut transcript = TranscriptDocument::new("budget-check", &character.id);
            transcript.local = Some(local.id.clone());
            let context = scene::resolve_prompt_context(&vault, &transcript).unwrap();
            let layers = PromptLayers {
                application_prompt: Some(DEFAULT_APPLICATION_PROMPT),
                user_persona: context.persona.as_deref(),
                scene_context: context.scene.as_deref(),
            };
            let base = build_prompt_with_memories(
                &character,
                layers,
                &transcript,
                query,
                &[],
                PromptBudget::default(),
            )
            .unwrap();
            assert!(
                base.estimated_input_tokens < 2500,
                "base={} scene={}",
                base.estimated_input_tokens,
                local.id
            );
            let built = build_prompt_with_memories(
                &character,
                layers,
                &transcript,
                query,
                &selected,
                PromptBudget::default(),
            )
            .unwrap();
            assert!(built.estimated_input_tokens + built.reserved_output_tokens <= 4096);
            println!(
                "{} / {}: base={}, with retrieval={}",
                expected, local.id, base.estimated_input_tokens, built.estimated_input_tokens
            );
        }
    }
}

#[test]
fn holmes_new_fact_survives_reopen_and_rebuild_without_contaminating_other_vaults() {
    // This tests durable retrieval, not model extraction. The record is deliberately authored here.
    let scratch = Scratch::new();
    let vault = scratch.install();
    let mut store = MemoryStore::open(&vault, "sherlock-holmes").unwrap();
    let mut note = MemoryRecord::new(
        "test-compass",
        MemoryType::Episodic,
        "The fictional visitor's missing compass has case token saffron kite.",
    );
    note.source_session_id = Some("synthetic-fixture".into());
    note.source_turn_ids = vec!["synthetic-user-turn".into()];
    store.create(&note).unwrap();
    drop(store);
    let reopened = Vault::open(vault.root()).unwrap();
    let mut store = MemoryStore::open(&reopened, "sherlock-holmes").unwrap();
    store.rebuild().unwrap();
    let found = retrieve(
        &mut store,
        "compass case token",
        RetrievalBudget::default(),
        &[],
    )
    .unwrap();
    assert!(found
        .iter()
        .any(|memory| memory.content.contains("saffron kite")));
    let other = Vault::create(scratch.0.join("other-vault")).unwrap();
    let mut other_store = MemoryStore::open(&other, "other-character").unwrap();
    assert!(retrieve(
        &mut other_store,
        "compass case token",
        RetrievalBudget::default(),
        &[]
    )
    .unwrap()
    .is_empty());
}
