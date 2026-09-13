use crate::storage::{
    atomic_write, parse_markdown, render_markdown, validate_id, StorageError, TranscriptDocument,
    Vault, STORAGE_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersonaNotes {
    pub schema_version: u32,
    pub body: String,
}

impl Default for PersonaNotes {
    fn default() -> Self {
        Self {
            schema_version: STORAGE_SCHEMA_VERSION,
            body: String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SceneSettings {
    pub schema_version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_local: Option<String>,
}

impl Default for SceneSettings {
    fn default() -> Self {
        Self {
            schema_version: STORAGE_SCHEMA_VERSION,
            default_local: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalRecord {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LocalSummary {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SessionLocalSelection {
    Default,
    None,
    Local { id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PromptContext {
    pub persona: Option<String>,
    pub scene: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct PersonaMetadata {
    schema_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct LocalMetadata {
    schema_version: u32,
    id: String,
    title: String,
}

pub fn load_persona(vault: &Vault) -> Result<PersonaNotes, StorageError> {
    let path = persona_path(vault)?;
    if !path.exists() {
        return Ok(PersonaNotes::default());
    }
    let (metadata, body): (PersonaMetadata, String) = parse_markdown(&fs::read(path)?)?;
    validate_schema(metadata.schema_version, "persona")?;
    Ok(PersonaNotes {
        schema_version: metadata.schema_version,
        body,
    })
}

pub fn save_persona(vault: &Vault, notes: &PersonaNotes) -> Result<(), StorageError> {
    validate_schema(notes.schema_version, "persona")?;
    let metadata = PersonaMetadata {
        schema_version: notes.schema_version,
    };
    atomic_write(
        &persona_path(vault)?,
        render_markdown(&metadata, &notes.body)?,
    )
}

pub fn load_scene_settings(vault: &Vault) -> Result<SceneSettings, StorageError> {
    let path = scene_path(vault)?;
    if !path.exists() {
        return Ok(SceneSettings::default());
    }
    let (settings, _body): (SceneSettings, String) = parse_markdown(&fs::read(path)?)?;
    validate_schema(settings.schema_version, "scene")?;
    Ok(SceneSettings {
        default_local: normalized_optional_id(settings.default_local.as_deref())?
            .map(str::to_owned),
        schema_version: settings.schema_version,
    })
}

pub fn save_scene_settings(vault: &Vault, settings: &SceneSettings) -> Result<(), StorageError> {
    validate_schema(settings.schema_version, "scene")?;
    let default_local =
        normalized_optional_id(settings.default_local.as_deref())?.map(str::to_owned);
    if let Some(id) = &default_local {
        ensure_local_exists(vault, id)?;
    }
    let stored = SceneSettings {
        schema_version: settings.schema_version,
        default_local,
    };
    atomic_write(&scene_path(vault)?, render_markdown(&stored, "")?)
}

pub fn list_locals(vault: &Vault) -> Result<Vec<LocalSummary>, StorageError> {
    let directory = locals_dir(vault)?;
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut locals = Vec::new();
    for entry in fs::read_dir(&directory)? {
        let path = entry?.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
            continue;
        }
        let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if validate_id(id).is_err() {
            continue;
        }
        if let Ok(record) = load_local(vault, id) {
            locals.push(LocalSummary {
                id: record.id,
                title: record.title,
            });
        }
    }
    locals.sort_by(|left, right| left.title.cmp(&right.title).then(left.id.cmp(&right.id)));
    Ok(locals)
}

pub fn load_local(vault: &Vault, id: &str) -> Result<LocalRecord, StorageError> {
    validate_id(id)?;
    let path = local_path(vault, id)?;
    if !path.exists() {
        return Err(missing_local(id));
    }
    let (metadata, body): (LocalMetadata, String) = parse_markdown(&fs::read(path)?)?;
    validate_schema(metadata.schema_version, "local")?;
    validate_id(&metadata.id)?;
    if metadata.id != id {
        return Err(StorageError::InvalidSchema(format!(
            "local file {id}.md id does not match its filename"
        )));
    }
    Ok(LocalRecord {
        schema_version: metadata.schema_version,
        id: metadata.id,
        title: metadata.title,
        body,
    })
}

pub fn save_local(vault: &Vault, record: &LocalRecord) -> Result<(), StorageError> {
    validate_schema(record.schema_version, "local")?;
    validate_id(&record.id)?;
    if record.title.trim().is_empty() {
        return Err(StorageError::InvalidSchema(
            "local title is required".into(),
        ));
    }
    let metadata = LocalMetadata {
        schema_version: record.schema_version,
        id: record.id.clone(),
        title: record.title.trim().to_owned(),
    };
    atomic_write(
        &local_path(vault, &record.id)?,
        render_markdown(&metadata, &record.body)?,
    )
}

pub fn delete_local(vault: &Vault, id: &str) -> Result<(), StorageError> {
    validate_id(id)?;
    let path = local_path(vault, id)?;
    if path.exists() {
        fs::remove_file(path)?;
    }
    let mut settings = load_scene_settings(vault)?;
    if settings.default_local.as_deref() == Some(id) {
        settings.default_local = None;
        save_scene_settings(vault, &settings)?;
    }
    Ok(())
}

pub fn selection_from_transcript(transcript: &TranscriptDocument) -> SessionLocalSelection {
    match transcript.local.as_deref() {
        None => SessionLocalSelection::Default,
        Some(value) if value.trim().is_empty() => SessionLocalSelection::None,
        Some(id) => SessionLocalSelection::Local { id: id.to_owned() },
    }
}

pub fn apply_selection(transcript: &mut TranscriptDocument, selection: &SessionLocalSelection) {
    transcript.local = match selection {
        SessionLocalSelection::Default => None,
        SessionLocalSelection::None => Some(String::new()),
        SessionLocalSelection::Local { id } => Some(id.clone()),
    };
}

pub fn validate_selection(
    vault: &Vault,
    selection: &SessionLocalSelection,
) -> Result<(), StorageError> {
    match selection {
        SessionLocalSelection::Default | SessionLocalSelection::None => Ok(()),
        SessionLocalSelection::Local { id } => {
            validate_id(id)?;
            ensure_local_exists(vault, id)
        }
    }
}

pub fn resolve_prompt_context(
    vault: &Vault,
    transcript: &TranscriptDocument,
) -> Result<PromptContext, StorageError> {
    let notes = load_persona(vault)?;
    let persona = notes.body.trim();
    let persona = if persona.is_empty() {
        None
    } else {
        Some(format!("User persona (who the user is):\n{persona}"))
    };
    let scene = match selection_from_transcript(transcript) {
        SessionLocalSelection::None => None,
        SessionLocalSelection::Default => match load_scene_settings(vault)?.default_local {
            Some(id) => render_scene(&load_local(vault, &id)?)?,
            None => None,
        },
        SessionLocalSelection::Local { id } => render_scene(&load_local(vault, &id)?)?,
    };
    Ok(PromptContext { persona, scene })
}

fn render_scene(record: &LocalRecord) -> Result<Option<String>, StorageError> {
    let title = record.title.trim();
    let body = record.body.trim();
    if title.is_empty() && body.is_empty() {
        return Ok(None);
    }
    let heading = if title.is_empty() {
        "Current scene:".to_owned()
    } else {
        format!("Current scene ({title}):")
    };
    if body.is_empty() {
        Ok(Some(heading))
    } else {
        Ok(Some(format!("{heading}\n{body}")))
    }
}

fn ensure_local_exists(vault: &Vault, id: &str) -> Result<(), StorageError> {
    let path = local_path(vault, id)?;
    if path.exists() {
        Ok(())
    } else {
        Err(missing_local(id))
    }
}

fn missing_local(id: &str) -> StorageError {
    StorageError::InvalidSchema(format!(
        "local {id} does not exist; add it to locals before using it"
    ))
}

fn validate_schema(version: u32, kind: &str) -> Result<(), StorageError> {
    if version != STORAGE_SCHEMA_VERSION {
        return Err(StorageError::InvalidSchema(format!(
            "unsupported {kind} version {version}"
        )));
    }
    Ok(())
}

fn normalized_optional_id(value: Option<&str>) -> Result<Option<&str>, StorageError> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        None => Ok(None),
        Some(id) => {
            validate_id(id)?;
            Ok(Some(id))
        }
    }
}

fn persona_path(vault: &Vault) -> Result<PathBuf, StorageError> {
    vault.resolve_relative("persona.md")
}

fn scene_path(vault: &Vault) -> Result<PathBuf, StorageError> {
    vault.resolve_relative("scene.md")
}

fn locals_dir(vault: &Vault) -> Result<PathBuf, StorageError> {
    vault.resolve_relative("locals")
}

fn local_path(vault: &Vault, id: &str) -> Result<PathBuf, StorageError> {
    validate_id(id)?;
    vault.resolve_relative(format!("locals/{id}.md"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{
        CharacterDefinition, MemoryRecord, MemoryType, TranscriptTurn, TurnRole, TurnStatus,
    };
    use std::path::PathBuf;

    fn test_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "tz-chatter-scene-{label}-{}",
            crate::storage::new_stable_id()
        ))
    }

    fn vault_with_character(label: &str) -> (PathBuf, Vault) {
        let root = test_root(label);
        let vault = Vault::create(&root).unwrap();
        vault
            .save_character(&CharacterDefinition::new("lyra", "Lyra", "You are Lyra."))
            .unwrap();
        (root, vault)
    }

    fn cafe() -> LocalRecord {
        LocalRecord {
            schema_version: STORAGE_SCHEMA_VERSION,
            id: "cafe".into(),
            title: "Evening cafe".into(),
            body: "A quiet corner table, rain on the windows.".into(),
        }
    }

    #[test]
    fn persona_round_trips_and_missing_file_is_empty() {
        let (root, vault) = vault_with_character("persona");
        assert_eq!(load_persona(&vault).unwrap(), PersonaNotes::default());
        let notes = PersonaNotes {
            schema_version: STORAGE_SCHEMA_VERSION,
            body: "I am Alex, Lyra's roommate.\nI prefer concise replies.".into(),
        };
        save_persona(&vault, &notes).unwrap();
        assert_eq!(load_persona(&vault).unwrap(), notes);
        let character = fs::read_to_string(vault.character_path().unwrap()).unwrap();
        assert!(!character.contains("Lyra's roommate"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn locals_round_trip_and_must_exist_before_default() {
        let (root, vault) = vault_with_character("locals");
        let error = save_scene_settings(
            &vault,
            &SceneSettings {
                schema_version: STORAGE_SCHEMA_VERSION,
                default_local: Some("cafe".into()),
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("add it to locals"));
        save_local(&vault, &cafe()).unwrap();
        save_scene_settings(
            &vault,
            &SceneSettings {
                schema_version: STORAGE_SCHEMA_VERSION,
                default_local: Some("cafe".into()),
            },
        )
        .unwrap();
        assert_eq!(
            list_locals(&vault).unwrap(),
            vec![LocalSummary {
                id: "cafe".into(),
                title: "Evening cafe".into(),
            }]
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resolve_uses_default_session_override_none_and_live_link() {
        let (root, vault) = vault_with_character("resolve");
        save_persona(
            &vault,
            &PersonaNotes {
                schema_version: STORAGE_SCHEMA_VERSION,
                body: "I am Alex.".into(),
            },
        )
        .unwrap();
        save_local(&vault, &cafe()).unwrap();
        save_local(
            &vault,
            &LocalRecord {
                schema_version: STORAGE_SCHEMA_VERSION,
                id: "apartment".into(),
                title: "Shared apartment".into(),
                body: "Late evening, dishes in the sink.".into(),
            },
        )
        .unwrap();
        save_scene_settings(
            &vault,
            &SceneSettings {
                schema_version: STORAGE_SCHEMA_VERSION,
                default_local: Some("cafe".into()),
            },
        )
        .unwrap();

        let mut transcript = TranscriptDocument::new("session", "lyra");
        let default_context = resolve_prompt_context(&vault, &transcript).unwrap();
        assert_eq!(
            default_context.persona.as_deref(),
            Some("User persona (who the user is):\nI am Alex.")
        );
        assert_eq!(
            default_context.scene.as_deref(),
            Some("Current scene (Evening cafe):\nA quiet corner table, rain on the windows.")
        );

        apply_selection(
            &mut transcript,
            &SessionLocalSelection::Local {
                id: "apartment".into(),
            },
        );
        let override_context = resolve_prompt_context(&vault, &transcript).unwrap();
        assert!(override_context
            .scene
            .as_deref()
            .unwrap()
            .contains("Shared apartment"));
        assert!(!override_context
            .scene
            .as_deref()
            .unwrap()
            .contains("Evening cafe"));

        apply_selection(&mut transcript, &SessionLocalSelection::None);
        assert!(resolve_prompt_context(&vault, &transcript)
            .unwrap()
            .scene
            .is_none());

        apply_selection(
            &mut transcript,
            &SessionLocalSelection::Local { id: "cafe".into() },
        );
        let mut updated = cafe();
        updated.body = "The cafe is closing; chairs stacked.".into();
        save_local(&vault, &updated).unwrap();
        let live = resolve_prompt_context(&vault, &transcript).unwrap();
        assert!(live.scene.as_deref().unwrap().contains("chairs stacked"));
        assert!(!live
            .scene
            .as_deref()
            .unwrap()
            .contains("rain on the windows"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_selected_local_fails_resolution() {
        let (root, vault) = vault_with_character("missing-local");
        let mut transcript = TranscriptDocument::new("session", "lyra");
        apply_selection(
            &mut transcript,
            &SessionLocalSelection::Local { id: "cafe".into() },
        );
        let error = resolve_prompt_context(&vault, &transcript).unwrap_err();
        assert!(error.to_string().contains("add it to locals"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn memory_writes_cannot_replace_persona_or_locals() {
        let (root, vault) = vault_with_character("isolation");
        save_persona(
            &vault,
            &PersonaNotes {
                schema_version: STORAGE_SCHEMA_VERSION,
                body: "I am Alex.".into(),
            },
        )
        .unwrap();
        save_local(&vault, &cafe()).unwrap();
        let persona_before = fs::read_to_string(persona_path(&vault).unwrap()).unwrap();
        let local_before = fs::read_to_string(local_path(&vault, "cafe").unwrap()).unwrap();
        vault
            .save_memory(&MemoryRecord::new(
                "topic",
                MemoryType::Semantic,
                "A memory about cafes.",
            ))
            .unwrap();
        assert_eq!(
            fs::read_to_string(persona_path(&vault).unwrap()).unwrap(),
            persona_before
        );
        assert_eq!(
            fs::read_to_string(local_path(&vault, "cafe").unwrap()).unwrap(),
            local_before
        );
        assert!(persona_path(&vault)
            .unwrap()
            .parent()
            .unwrap()
            .join("memories")
            .join("semantic")
            .join("topic.md")
            .exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn transcript_local_round_trips_follow_default_none_and_id() {
        let (root, vault) = vault_with_character("transcript-local");
        save_local(&vault, &cafe()).unwrap();
        let mut transcript = TranscriptDocument::new("session", "lyra");
        transcript.created_at = "1".into();
        transcript.updated_at = "1".into();
        transcript.turns = vec![TranscriptTurn {
            id: "user-1".into(),
            timestamp: "1".into(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: "Hello".into(),
        }];
        vault.save_transcript(&transcript).unwrap();
        assert_eq!(vault.load_transcript("session").unwrap().local, None);

        apply_selection(&mut transcript, &SessionLocalSelection::None);
        vault.save_transcript(&transcript).unwrap();
        assert_eq!(
            vault.load_transcript("session").unwrap().local.as_deref(),
            Some("")
        );

        apply_selection(
            &mut transcript,
            &SessionLocalSelection::Local { id: "cafe".into() },
        );
        vault.save_transcript(&transcript).unwrap();
        assert_eq!(
            vault.load_transcript("session").unwrap().local.as_deref(),
            Some("cafe")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deleting_default_local_clears_scene_settings() {
        let (root, vault) = vault_with_character("delete-default");
        save_local(&vault, &cafe()).unwrap();
        save_scene_settings(
            &vault,
            &SceneSettings {
                schema_version: STORAGE_SCHEMA_VERSION,
                default_local: Some("cafe".into()),
            },
        )
        .unwrap();
        delete_local(&vault, "cafe").unwrap();
        assert!(load_scene_settings(&vault).unwrap().default_local.is_none());
        assert!(list_locals(&vault).unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
