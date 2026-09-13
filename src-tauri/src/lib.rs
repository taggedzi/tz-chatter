use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{
    ipc::Channel,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WindowEvent,
};
use tauri_plugin_notification::NotificationExt;
use tokio_util::sync::CancellationToken;

pub mod characters;
pub mod connections;
pub mod conversation;
pub mod embeddings;
pub mod extraction;
pub mod generation;
pub mod initiative;
pub mod memory;
pub mod portability;
pub mod prompt;
pub mod providers;
pub mod reconciliation;
pub mod retrieval;
pub mod scene;
pub mod storage;

#[derive(Default)]
struct RuntimeState {
    work: Mutex<HashMap<String, (u64, CancellationToken)>>,
    generations: Mutex<HashMap<String, u64>>,
    schedulers: Mutex<HashMap<String, (u64, CancellationToken)>>,
    background: Mutex<HashMap<String, CancellationToken>>,
    model_gate: tokio::sync::Mutex<()>,
}

impl RuntimeState {
    fn next_generation(&self, key: &str) -> u64 {
        let mut generations = self
            .generations
            .lock()
            .expect("generation state mutex poisoned");
        let generation = generations
            .get(key)
            .copied()
            .unwrap_or_default()
            .saturating_add(1);
        generations.insert(key.to_owned(), generation);
        generation
    }

    fn begin(&self, key: &str) -> (u64, CancellationToken) {
        self.cancel_background();
        let mut work = self.work.lock().expect("runtime state mutex poisoned");
        if let Some((_, previous)) = work.get(key) {
            previous.cancel();
        }
        let generation = self.next_generation(key);
        let token = CancellationToken::new();
        work.insert(key.to_owned(), (generation, token.clone()));
        (generation, token)
    }

    fn finish(&self, key: &str, generation: u64) {
        let mut work = self.work.lock().expect("runtime state mutex poisoned");
        if work
            .get(key)
            .is_some_and(|(current_generation, _)| *current_generation == generation)
        {
            work.remove(key);
        }
    }

    fn try_begin_child(
        &self,
        key: &str,
        parent: &CancellationToken,
    ) -> Option<(u64, CancellationToken)> {
        let mut work = self.work.lock().expect("runtime state mutex poisoned");
        if work.contains_key(key) {
            return None;
        }
        let generation = self.next_generation(key);
        let token = parent.child_token();
        work.insert(key.to_owned(), (generation, token.clone()));
        Some((generation, token))
    }

    fn start_scheduler(&self, key: &str) -> (u64, CancellationToken) {
        let mut schedulers = self
            .schedulers
            .lock()
            .expect("scheduler state mutex poisoned");
        if let Some((_, previous)) = schedulers.get(key) {
            previous.cancel();
        }
        let generation = schedulers
            .get(key)
            .map(|(generation, _)| generation.saturating_add(1))
            .unwrap_or(1);
        let token = CancellationToken::new();
        schedulers.insert(key.to_owned(), (generation, token.clone()));
        (generation, token)
    }

    fn finish_scheduler(&self, key: &str, generation: u64) {
        let mut schedulers = self
            .schedulers
            .lock()
            .expect("scheduler state mutex poisoned");
        if schedulers
            .get(key)
            .is_some_and(|(current_generation, _)| *current_generation == generation)
        {
            schedulers.remove(key);
        }
    }

    fn stop_scheduler(&self, key: &str) -> bool {
        let mut schedulers = self
            .schedulers
            .lock()
            .expect("scheduler state mutex poisoned");
        schedulers
            .remove(key)
            .map(|(_, token)| {
                token.cancel();
                true
            })
            .unwrap_or(false)
    }

    fn cancel(&self, key: &str) -> bool {
        let work = self.work.lock().expect("runtime state mutex poisoned");
        if let Some((_, token)) = work.get(key) {
            token.cancel();
            true
        } else {
            false
        }
    }

    fn begin_background(&self, key: &str) -> Option<CancellationToken> {
        if !self
            .work
            .lock()
            .expect("runtime state mutex poisoned")
            .is_empty()
        {
            return None;
        }
        let mut background = self
            .background
            .lock()
            .expect("background state mutex poisoned");
        if background.contains_key(key) {
            return None;
        }
        let token = CancellationToken::new();
        background.insert(key.to_owned(), token.clone());
        drop(background);
        if !self
            .work
            .lock()
            .expect("runtime state mutex poisoned")
            .is_empty()
        {
            self.finish_background(key);
            token.cancel();
            return None;
        }
        Some(token)
    }

    fn finish_background(&self, key: &str) {
        self.background
            .lock()
            .expect("background state mutex poisoned")
            .remove(key);
    }

    fn cancel_background(&self) {
        let mut background = self
            .background
            .lock()
            .expect("background state mutex poisoned");
        for (_, token) in background.drain() {
            token.cancel();
        }
    }

    fn resource_available(&self) -> bool {
        let foreground_idle = self
            .work
            .lock()
            .expect("runtime state mutex poisoned")
            .is_empty();
        if !foreground_idle {
            return false;
        }
        self.background
            .lock()
            .expect("background state mutex poisoned")
            .is_empty()
    }
}

#[derive(Debug, Serialize)]
struct AppInfo {
    name: &'static str,
    version: &'static str,
    stage: &'static str,
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        name: "tz-chatter",
        version: env!("CARGO_PKG_VERSION"),
        stage: "desktop shell",
    }
}

fn hide_window_instead_of_closing(prevent_close: impl FnOnce(), hide: impl FnOnce()) {
    prevent_close();
    hide();
}

fn open_character_vault(
    vault_root: impl AsRef<std::path::Path>,
    expected_character_id: &str,
) -> Result<(storage::Vault, storage::CharacterDefinition), String> {
    if expected_character_id.trim().is_empty() {
        return Err("load a character vault before running character-scoped commands".into());
    }
    let vault = storage::Vault::open(&vault_root).map_err(String::from)?;
    let character = vault.load_character().map_err(String::from)?;
    if character.id != expected_character_id {
        return Err(format!(
            "vault character {} does not match requested character {expected_character_id}",
            character.id
        ));
    }
    Ok((vault, character))
}

fn active_provider(app: &AppHandle) -> Result<providers::ProviderConfig, String> {
    let settings_path = providers::settings_path(
        &app.path()
            .app_config_dir()
            .map_err(|error| error.to_string())?,
    );
    let settings = providers::load_settings(&settings_path).map_err(String::from)?;
    let active_id = settings
        .active_provider_id
        .ok_or_else(|| "no active provider is configured".to_owned())?;
    settings
        .providers
        .into_iter()
        .find(|provider| provider.id == active_id)
        .ok_or_else(|| "the active provider configuration was not found".to_owned())
}

fn app_config_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path()
        .app_config_dir()
        .map_err(|error| error.to_string())
}

fn application_prompt_text(app: &AppHandle) -> String {
    let Ok(dir) = app_config_dir(app) else {
        return characters::DEFAULT_APPLICATION_PROMPT.to_owned();
    };
    characters::load_application_prompt(&characters::prompt_path(&dir))
        .map(|prompt| prompt.text)
        .unwrap_or_else(|_| characters::DEFAULT_APPLICATION_PROMPT.to_owned())
}

fn remember_character_vault(app: &AppHandle, vault_root: &str) {
    if let Ok(dir) = app_config_dir(app) {
        let _ = characters::add_existing(&characters::library_path(&dir), vault_root);
    }
}

#[tauri::command]
fn provider_capabilities(kind: providers::ProviderKind) -> providers::ProviderCapabilities {
    providers::ProviderCapabilities::baseline(&kind)
}

#[tauri::command]
fn initiative_snapshot(
    vault_root: String,
    character_id: String,
    now: i64,
    model_available: bool,
    resource_available: bool,
    utc_offset_minutes: i16,
) -> Result<initiative::InitiativeSnapshot, String> {
    let (vault, _) = open_character_vault(&vault_root, &character_id)?;
    initiative::InitiativeStore::open(&vault, &character_id)
        .map_err(String::from)?
        .evaluate(
            now,
            initiative::InitiativeContext {
                active_character: true,
                model_available,
                resource_available,
                utc_offset_minutes,
            },
        )
        .map_err(String::from)
}

#[tauri::command]
fn initiative_save_settings(
    vault_root: String,
    character_id: String,
    settings: initiative::InitiativeSettings,
) -> Result<(), String> {
    let (vault, _) = open_character_vault(&vault_root, &character_id)?;
    let store = initiative::InitiativeStore::open(&vault, &character_id).map_err(String::from)?;
    store.save_settings(&settings).map_err(String::from)
}

#[tauri::command]
fn initiative_scheduler_start(
    app: AppHandle,
    state: tauri::State<'_, RuntimeState>,
    vault_root: String,
    character_id: String,
    session_id: String,
    utc_offset_minutes: i16,
) -> Result<(), String> {
    let (vault, character) = open_character_vault(&vault_root, &character_id)?;
    let settings_path = providers::settings_path(
        &app.path()
            .app_config_dir()
            .map_err(|error| error.to_string())?,
    );
    let provider_settings = providers::load_settings(&settings_path).map_err(String::from)?;
    let active_provider_id = provider_settings
        .active_provider_id
        .ok_or_else(|| "save an active provider before enabling initiative".to_owned())?;
    let provider = provider_settings
        .providers
        .into_iter()
        .find(|candidate| candidate.id == active_provider_id)
        .ok_or_else(|| "the active provider configuration was not found".to_owned())?;
    let initiative_settings = initiative::InitiativeStore::open(&vault, &character_id)
        .map_err(String::from)?
        .settings()
        .map_err(String::from)?;
    if !initiative_settings.enabled {
        return Err("initiative is disabled; enable it before starting the scheduler".into());
    }
    let scheduler_key = format!("initiative:{character_id}:{session_id}");
    let work_key = format!("{character_id}:{session_id}");
    let (scheduler_generation, scheduler_token) = state.start_scheduler(&scheduler_key);
    let app_for_task = app.clone();
    tauri::async_runtime::spawn(async move {
        run_initiative_scheduler(InitiativeSchedulerArgs {
            app: app_for_task,
            scheduler_key,
            scheduler_generation,
            scheduler_token,
            work_key,
            vault_root,
            character,
            provider,
            session_id,
            utc_offset_minutes,
        })
        .await;
    });
    Ok(())
}

#[tauri::command]
fn initiative_scheduler_stop(
    state: tauri::State<'_, RuntimeState>,
    character_id: String,
    session_id: String,
) -> bool {
    state.stop_scheduler(&format!("initiative:{character_id}:{session_id}"))
}

struct InitiativeSchedulerArgs {
    app: AppHandle,
    scheduler_key: String,
    scheduler_generation: u64,
    scheduler_token: CancellationToken,
    work_key: String,
    vault_root: String,
    character: storage::CharacterDefinition,
    provider: providers::ProviderConfig,
    session_id: String,
    utc_offset_minutes: i16,
}

async fn run_initiative_scheduler(args: InitiativeSchedulerArgs) {
    let InitiativeSchedulerArgs {
        app,
        scheduler_key,
        scheduler_generation,
        scheduler_token,
        work_key,
        vault_root,
        character,
        provider,
        session_id,
        utc_offset_minutes,
    } = args;
    let mut last_tick = unix_now();
    loop {
        tokio::select! {
            _ = scheduler_token.cancelled() => break,
            _ = tokio::time::sleep(Duration::from_secs(30)) => {}
        }
        if scheduler_token.is_cancelled() {
            break;
        }
        let vault = match storage::Vault::open(&vault_root) {
            Ok(vault) => vault,
            Err(_) => continue,
        };
        let store = match initiative::InitiativeStore::open(&vault, &character.id) {
            Ok(store) => store,
            Err(_) => continue,
        };
        let now = unix_now();
        if now.saturating_sub(last_tick) > 90 {
            let _ = store.record_resume(now);
            last_tick = now;
            continue;
        }
        last_tick = now;
        let before = match store.state() {
            Ok(state) => state,
            Err(_) => continue,
        };
        if before.unanswered {
            let settings = match store.settings() {
                Ok(settings) => settings,
                Err(_) => continue,
            };
            if before
                .last_initiative_at
                .is_some_and(|sent| now.saturating_sub(sent) >= settings.cooldown_seconds)
            {
                let _ = store.record_initiative_ignored(now);
            }
        }
        let model_available = connections::ProviderClient::default()
            .health(&provider)
            .await
            .map(|health| health.reachable)
            .unwrap_or(false);
        let runtime = app.state::<RuntimeState>();
        let snapshot = match store.evaluate(
            now,
            initiative::InitiativeContext {
                active_character: true,
                model_available,
                resource_available: runtime.resource_available(),
                utc_offset_minutes,
            },
        ) {
            Ok(snapshot) => snapshot,
            Err(_) => continue,
        };
        if !snapshot.decision.eligible {
            continue;
        }
        let Some((generation, cancellation)) = runtime.try_begin_child(&work_key, &scheduler_token)
        else {
            continue;
        };
        let request = initiative::InitiativeRequest {
            character: character.clone(),
            provider: provider.clone(),
            session_id: session_id.clone(),
            request_id: storage::new_stable_id(),
            topic_context: String::new(),
            generation,
            started_at: now,
            application_prompt: application_prompt_text(&app),
        };
        let latest_user_activity_at = snapshot.state.last_user_activity_at;
        let model_guard = runtime.model_gate.lock().await;
        let outcome = conversation::ConversationService::with_provider_client()
            .send_initiative(
                &vault,
                request,
                generation,
                latest_user_activity_at,
                cancellation,
            )
            .await;
        drop(model_guard);
        runtime.finish(&work_key, generation);
        match outcome {
            Ok(outcome)
                if matches!(
                    outcome.status,
                    conversation::InitiativeDeliveryStatus::Delivered
                ) =>
            {
                let _ = store.record_initiative_sent(now);
                let _ = app.emit("initiative-delivered", &outcome);
                if let Some(turn) = outcome.initiative_turn {
                    let notifications_enabled = store
                        .settings()
                        .map(|settings| settings.notifications_enabled)
                        .unwrap_or(false);
                    if notifications_enabled {
                        let _ = app
                            .notification()
                            .builder()
                            .title(format!("{} reached out", character.name))
                            .body(&turn.content)
                            .show();
                    }
                }
            }
            Ok(outcome)
                if matches!(
                    outcome.status,
                    conversation::InitiativeDeliveryStatus::Silenced
                ) =>
            {
                let _ = store.record_initiative_silenced(now);
            }
            _ => {}
        }
    }
    app.state::<RuntimeState>()
        .finish_scheduler(&scheduler_key, scheduler_generation);
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or_default()
}

#[tauri::command]
fn initiative_record_user_activity(
    vault_root: String,
    character_id: String,
    now: i64,
) -> Result<initiative::InitiativeState, String> {
    let (vault, _) = open_character_vault(vault_root, &character_id)?;
    initiative::InitiativeStore::open(&vault, &character_id)
        .map_err(String::from)?
        .record_user_activity(now)
        .map_err(String::from)
}

#[tauri::command]
fn initiative_record_sent(
    vault_root: String,
    character_id: String,
    now: i64,
) -> Result<initiative::InitiativeState, String> {
    let (vault, _) = open_character_vault(vault_root, &character_id)?;
    initiative::InitiativeStore::open(&vault, &character_id)
        .map_err(String::from)?
        .record_initiative_sent(now)
        .map_err(String::from)
}

#[tauri::command]
fn initiative_record_ignored(
    vault_root: String,
    character_id: String,
    now: i64,
) -> Result<initiative::InitiativeState, String> {
    let (vault, _) = open_character_vault(vault_root, &character_id)?;
    initiative::InitiativeStore::open(&vault, &character_id)
        .map_err(String::from)?
        .record_initiative_ignored(now)
        .map_err(String::from)
}

#[tauri::command]
fn initiative_record_resume(
    vault_root: String,
    character_id: String,
    now: i64,
) -> Result<initiative::InitiativeState, String> {
    let (vault, _) = open_character_vault(vault_root, &character_id)?;
    initiative::InitiativeStore::open(&vault, &character_id)
        .map_err(String::from)?
        .record_resume(now)
        .map_err(String::from)
}

#[tauri::command]
fn validate_provider_config(
    config: providers::ProviderConfig,
) -> Result<providers::RedactedProviderConfig, String> {
    providers::validate_config(&config).map_err(String::from)?;
    Ok((&config).into())
}

#[tauri::command]
fn load_provider_settings(app: AppHandle) -> Result<providers::ProviderSettings, String> {
    let path = providers::settings_path(
        &app.path()
            .app_config_dir()
            .map_err(|error| error.to_string())?,
    );
    providers::load_settings(&path).map_err(String::from)
}

#[tauri::command]
fn save_provider_settings(
    app: AppHandle,
    settings: providers::ProviderSettings,
) -> Result<(), String> {
    let path = providers::settings_path(
        &app.path()
            .app_config_dir()
            .map_err(|error| error.to_string())?,
    );
    providers::save_settings(&path, &settings).map_err(String::from)
}

#[tauri::command]
async fn provider_health(
    config: providers::ProviderConfig,
) -> Result<providers::HealthResponse, String> {
    connections::ProviderClient::default()
        .health(&config)
        .await
        .map_err(String::from)
}

#[tauri::command]
async fn provider_discover(
    config: providers::ProviderConfig,
) -> Result<providers::DiscoveryResponse, String> {
    connections::ProviderClient::default()
        .discover(&config)
        .await
        .map_err(String::from)
}

#[tauri::command]
async fn provider_embed(
    config: providers::ProviderConfig,
    request: providers::EmbeddingRequest,
) -> Result<providers::EmbeddingResponse, String> {
    connections::ProviderClient::default()
        .embed(&config, &request)
        .await
        .map_err(String::from)
}

#[tauri::command]
fn character_library_list(app: AppHandle) -> Result<characters::CharacterLibraryView, String> {
    let path = characters::library_path(app_config_dir(&app)?);
    characters::list_library(&path).map_err(String::from)
}

#[tauri::command]
fn character_library_add(
    app: AppHandle,
    vault_root: String,
) -> Result<characters::CharacterLibraryItem, String> {
    let path = characters::library_path(app_config_dir(&app)?);
    characters::add_existing(&path, vault_root).map_err(String::from)
}

#[tauri::command]
fn character_library_create(
    app: AppHandle,
    parent_dir: String,
    name: String,
) -> Result<characters::CharacterLibraryItem, String> {
    let path = characters::library_path(app_config_dir(&app)?);
    characters::create_character(&path, parent_dir, &name).map_err(String::from)
}

#[tauri::command]
fn character_library_remove(app: AppHandle, vault_root: String) -> Result<(), String> {
    let path = characters::library_path(app_config_dir(&app)?);
    characters::remove_from_library(&path, vault_root).map_err(String::from)
}

#[tauri::command]
fn character_load(vault_root: String) -> Result<storage::CharacterDefinition, String> {
    characters::load_character(vault_root).map_err(String::from)
}

#[tauri::command]
fn character_save(
    app: AppHandle,
    vault_root: String,
    character: storage::CharacterDefinition,
) -> Result<storage::CharacterDefinition, String> {
    let path = characters::library_path(app_config_dir(&app)?);
    characters::save_character(&path, vault_root, character).map_err(String::from)
}

#[tauri::command]
fn character_portrait_load(vault_root: String) -> Result<Option<String>, String> {
    match characters::load_portrait(vault_root).map_err(String::from)? {
        Some(bytes) => Ok(Some(characters::portrait_data_url(&bytes))),
        None => Ok(None),
    }
}

#[tauri::command]
fn character_portrait_set(vault_root: String, data_base64: String) -> Result<(), String> {
    let bytes = characters::decode_portrait_base64(&data_base64).map_err(String::from)?;
    characters::set_portrait(vault_root, &bytes).map_err(String::from)
}

#[tauri::command]
fn character_portrait_clear(vault_root: String) -> Result<(), String> {
    characters::clear_portrait(vault_root).map_err(String::from)
}

#[tauri::command]
fn generation_load(vault_root: String) -> Result<generation::CharacterGeneration, String> {
    let vault = storage::Vault::open(&vault_root).map_err(String::from)?;
    generation::load(&vault).map_err(|error| error.to_string())
}

#[tauri::command]
fn generation_save(
    vault_root: String,
    settings: generation::CharacterGeneration,
) -> Result<generation::CharacterGeneration, String> {
    let vault = storage::Vault::open(&vault_root).map_err(String::from)?;
    generation::save(&vault, &settings).map_err(|error| error.to_string())?;
    generation::load(&vault).map_err(|error| error.to_string())
}

#[tauri::command]
fn application_prompt_load(app: AppHandle) -> Result<characters::ApplicationPrompt, String> {
    let path = characters::prompt_path(app_config_dir(&app)?);
    characters::load_application_prompt(&path).map_err(String::from)
}

#[tauri::command]
fn application_prompt_save(
    app: AppHandle,
    prompt: characters::ApplicationPrompt,
) -> Result<(), String> {
    let path = characters::prompt_path(app_config_dir(&app)?);
    characters::save_application_prompt(&path, &prompt).map_err(String::from)
}

#[tauri::command]
fn persona_load(vault_root: String) -> Result<scene::PersonaNotes, String> {
    let vault = storage::Vault::open(&vault_root).map_err(String::from)?;
    scene::load_persona(&vault).map_err(String::from)
}

#[tauri::command]
fn persona_save(
    vault_root: String,
    notes: scene::PersonaNotes,
) -> Result<scene::PersonaNotes, String> {
    let vault = storage::Vault::open(&vault_root).map_err(String::from)?;
    scene::save_persona(&vault, &notes).map_err(String::from)?;
    scene::load_persona(&vault).map_err(String::from)
}

#[tauri::command]
fn scene_settings_load(vault_root: String) -> Result<scene::SceneSettings, String> {
    let vault = storage::Vault::open(&vault_root).map_err(String::from)?;
    scene::load_scene_settings(&vault).map_err(String::from)
}

#[tauri::command]
fn scene_settings_save(
    vault_root: String,
    settings: scene::SceneSettings,
) -> Result<scene::SceneSettings, String> {
    let vault = storage::Vault::open(&vault_root).map_err(String::from)?;
    scene::save_scene_settings(&vault, &settings).map_err(String::from)?;
    scene::load_scene_settings(&vault).map_err(String::from)
}

#[tauri::command]
fn locals_list(vault_root: String) -> Result<Vec<scene::LocalSummary>, String> {
    let vault = storage::Vault::open(&vault_root).map_err(String::from)?;
    scene::list_locals(&vault).map_err(String::from)
}

#[tauri::command]
fn local_load(vault_root: String, id: String) -> Result<scene::LocalRecord, String> {
    let vault = storage::Vault::open(&vault_root).map_err(String::from)?;
    scene::load_local(&vault, &id).map_err(String::from)
}

#[tauri::command]
fn local_save(
    vault_root: String,
    record: scene::LocalRecord,
) -> Result<scene::LocalRecord, String> {
    let vault = storage::Vault::open(&vault_root).map_err(String::from)?;
    scene::save_local(&vault, &record).map_err(String::from)?;
    scene::load_local(&vault, &record.id).map_err(String::from)
}

#[tauri::command]
fn local_delete(vault_root: String, id: String) -> Result<(), String> {
    let vault = storage::Vault::open(&vault_root).map_err(String::from)?;
    scene::delete_local(&vault, &id).map_err(String::from)
}

#[derive(Clone, Copy)]
enum ChatTurnKind {
    Send,
    Retry,
    Regenerate,
    Continue,
    EditLastUser,
}

#[tauri::command]
async fn conversation_send(
    app: AppHandle,
    state: tauri::State<'_, RuntimeState>,
    vault_root: String,
    snapshot: conversation::RequestSnapshot,
    on_event: Channel<providers::ChatStreamEvent>,
) -> Result<conversation::ConversationOutcome, String> {
    conversation_chat_turn(
        app,
        state,
        vault_root,
        snapshot,
        on_event,
        ChatTurnKind::Send,
    )
    .await
}

#[tauri::command]
fn conversation_resume(
    app: AppHandle,
    vault_root: String,
    expected_character_id: Option<String>,
) -> Result<conversation::ConversationResume, String> {
    let vault = storage::Vault::open(&vault_root).map_err(String::from)?;
    let resumed =
        conversation::ConversationService::resume(&vault, expected_character_id.as_deref())
            .map_err(String::from)?;
    remember_character_vault(&app, &vault_root);
    if let Ok(provider) = active_provider(&app) {
        schedule_extraction(app, &vault_root, provider, resumed.character.clone());
    }
    Ok(resumed)
}

#[tauri::command]
fn conversation_list_sessions(
    vault_root: String,
    include_archived: bool,
) -> Result<Vec<conversation::SessionSummary>, String> {
    let vault = storage::Vault::open(&vault_root).map_err(String::from)?;
    conversation::ConversationService::list_sessions_filtered(&vault, include_archived)
        .map_err(String::from)
}

#[tauri::command]
fn conversation_search_sessions(
    vault_root: String,
    query: String,
) -> Result<Vec<conversation::SessionSummary>, String> {
    let vault = storage::Vault::open(&vault_root).map_err(String::from)?;
    conversation::ConversationService::search_sessions(&vault, &query).map_err(String::from)
}

#[tauri::command]
fn conversation_rename_session(
    vault_root: String,
    session_id: String,
    title: String,
) -> Result<storage::TranscriptDocument, String> {
    let vault = storage::Vault::open(&vault_root).map_err(String::from)?;
    conversation::ConversationService::rename_session(&vault, &session_id, &title)
        .map_err(String::from)
}

#[tauri::command]
fn conversation_set_session_archived(
    vault_root: String,
    session_id: String,
    archived: bool,
) -> Result<storage::TranscriptDocument, String> {
    let vault = storage::Vault::open(&vault_root).map_err(String::from)?;
    conversation::ConversationService::set_session_archived(&vault, &session_id, archived)
        .map_err(String::from)
}

#[tauri::command]
fn conversation_open_session(
    vault_root: String,
    session_id: String,
) -> Result<conversation::ConversationResume, String> {
    let vault = storage::Vault::open(&vault_root).map_err(String::from)?;
    conversation::ConversationService::open_session(&vault, &session_id).map_err(String::from)
}

#[tauri::command]
fn conversation_start_session(
    vault_root: String,
) -> Result<conversation::ConversationResume, String> {
    let vault = storage::Vault::open(&vault_root).map_err(String::from)?;
    conversation::ConversationService::start_session(&vault).map_err(String::from)
}

#[tauri::command]
fn conversation_set_session_local(
    vault_root: String,
    session_id: String,
    selection: scene::SessionLocalSelection,
) -> Result<storage::TranscriptDocument, String> {
    let vault = storage::Vault::open(&vault_root).map_err(String::from)?;
    conversation::ConversationService::set_session_local(&vault, &session_id, selection)
        .map_err(String::from)
}

#[tauri::command]
async fn conversation_retry(
    app: AppHandle,
    state: tauri::State<'_, RuntimeState>,
    vault_root: String,
    snapshot: conversation::RequestSnapshot,
    on_event: Channel<providers::ChatStreamEvent>,
) -> Result<conversation::ConversationOutcome, String> {
    conversation_chat_turn(
        app,
        state,
        vault_root,
        snapshot,
        on_event,
        ChatTurnKind::Retry,
    )
    .await
}

#[tauri::command]
async fn conversation_regenerate(
    app: AppHandle,
    state: tauri::State<'_, RuntimeState>,
    vault_root: String,
    snapshot: conversation::RequestSnapshot,
    on_event: Channel<providers::ChatStreamEvent>,
) -> Result<conversation::ConversationOutcome, String> {
    conversation_chat_turn(
        app,
        state,
        vault_root,
        snapshot,
        on_event,
        ChatTurnKind::Regenerate,
    )
    .await
}

#[tauri::command]
async fn conversation_continue(
    app: AppHandle,
    state: tauri::State<'_, RuntimeState>,
    vault_root: String,
    snapshot: conversation::RequestSnapshot,
    on_event: Channel<providers::ChatStreamEvent>,
) -> Result<conversation::ConversationOutcome, String> {
    conversation_chat_turn(
        app,
        state,
        vault_root,
        snapshot,
        on_event,
        ChatTurnKind::Continue,
    )
    .await
}

#[tauri::command]
async fn conversation_edit_last_user(
    app: AppHandle,
    state: tauri::State<'_, RuntimeState>,
    vault_root: String,
    snapshot: conversation::RequestSnapshot,
    on_event: Channel<providers::ChatStreamEvent>,
) -> Result<conversation::ConversationOutcome, String> {
    conversation_chat_turn(
        app,
        state,
        vault_root,
        snapshot,
        on_event,
        ChatTurnKind::EditLastUser,
    )
    .await
}

async fn conversation_chat_turn(
    app: AppHandle,
    state: tauri::State<'_, RuntimeState>,
    vault_root: String,
    mut snapshot: conversation::RequestSnapshot,
    on_event: Channel<providers::ChatStreamEvent>,
    kind: ChatTurnKind,
) -> Result<conversation::ConversationOutcome, String> {
    let (vault, canonical_character) = open_character_vault(&vault_root, &snapshot.character.id)?;
    snapshot.character = canonical_character;
    snapshot.application_prompt = application_prompt_text(&app);
    let _ = initiative::InitiativeStore::open(&vault, &snapshot.character.id)
        .and_then(|store| store.record_user_activity(unix_now()));
    let key = format!("{}:{}", snapshot.character.id, snapshot.session_id);
    let extraction_provider = snapshot.provider.clone();
    let extraction_character = snapshot.character.clone();
    let use_hybrid = snapshot.use_hybrid_retrieval;
    let (generation, cancellation) = state.begin(&key);
    let sink: connections::StreamSink = Arc::new(move |event| {
        let _ = on_event.send(event);
    });
    let service = conversation::ConversationService::with_provider_client();
    let model_guard = state.model_gate.lock().await;
    let result = match kind {
        ChatTurnKind::Send => {
            service
                .send_streaming(&vault, snapshot, cancellation, Some(sink))
                .await
        }
        ChatTurnKind::Retry => {
            service
                .retry_streaming(&vault, snapshot, cancellation, Some(sink))
                .await
        }
        ChatTurnKind::Regenerate => {
            service
                .regenerate_streaming(&vault, snapshot, cancellation, Some(sink))
                .await
        }
        ChatTurnKind::Continue => {
            service
                .continue_reply_streaming(&vault, snapshot, cancellation, Some(sink))
                .await
        }
        ChatTurnKind::EditLastUser => {
            service
                .edit_last_user_streaming(&vault, snapshot, cancellation, Some(sink))
                .await
        }
    }
    .map_err(String::from);
    drop(model_guard);
    state.finish(&key, generation);
    if let Ok(outcome) = &result {
        if outcome.assistant_turn.status == storage::TurnStatus::Complete {
            schedule_extraction(
                app.clone(),
                &vault_root,
                extraction_provider.clone(),
                extraction_character.clone(),
            );
        }
        if should_schedule_embedding(use_hybrid, &extraction_provider, outcome) {
            schedule_embedding(app, &vault_root, extraction_provider, extraction_character);
        }
    }
    result
}

fn schedule_extraction(
    app: AppHandle,
    vault_root: &str,
    provider: providers::ProviderConfig,
    character: storage::CharacterDefinition,
) {
    let vault_root = vault_root.to_owned();
    tauri::async_runtime::spawn(async move {
        let key = format!("extraction:{}", character.id);
        let runtime = app.state::<RuntimeState>();
        let Some(cancellation) = runtime.begin_background(&key) else {
            return;
        };
        let model_guard = runtime.model_gate.lock().await;
        let transport: Arc<dyn conversation::ChatTransport> =
            Arc::new(connections::ProviderClient::default());
        let result = process_extraction_queue(
            vault_root.clone(),
            provider.clone(),
            character.clone(),
            cancellation,
            transport,
        )
        .await;
        drop(model_guard);
        runtime.finish_background(&key);
        match result {
            Ok(true) => schedule_embedding(app, &vault_root, provider, character),
            Ok(false) => {}
            Err(error) => eprintln!("background extraction did not complete: {error}"),
        }
    });
}

fn should_schedule_embedding(
    use_hybrid: bool,
    provider: &providers::ProviderConfig,
    outcome: &conversation::ConversationOutcome,
) -> bool {
    use_hybrid
        && conversation::embedding_model_configured(provider)
        && outcome
            .context_inspection
            .fallback_reason
            .as_deref()
            .is_some_and(|reason| reason.contains(conversation::EMBEDDING_INDEX_REBUILDING))
}

fn schedule_embedding(
    app: AppHandle,
    vault_root: &str,
    provider: providers::ProviderConfig,
    character: storage::CharacterDefinition,
) {
    if !conversation::embedding_model_configured(&provider) {
        return;
    }
    let vault_root = vault_root.to_owned();
    tauri::async_runtime::spawn(async move {
        let key = format!("embedding:{}", character.id);
        let runtime = app.state::<RuntimeState>();
        let Some(cancellation) = runtime.begin_background(&key) else {
            return;
        };
        let model_guard = runtime.model_gate.lock().await;
        let transport: Arc<dyn conversation::ChatTransport> =
            Arc::new(connections::ProviderClient::default());
        let result =
            process_embedding_rebuild(vault_root, provider, character, cancellation, transport)
                .await;
        drop(model_guard);
        runtime.finish_background(&key);
        if let Err(error) = result {
            eprintln!("background embedding rebuild did not complete: {error}");
        }
    });
}

async fn process_extraction_queue(
    vault_root: String,
    provider: providers::ProviderConfig,
    character: storage::CharacterDefinition,
    cancellation: CancellationToken,
    transport: Arc<dyn conversation::ChatTransport>,
) -> Result<bool, String> {
    tokio::task::yield_now().await;
    let (vault, _) = open_character_vault(vault_root, &character.id)?;
    let mut queue = extraction::ExtractionQueue::open(&vault, &character.id)
        .map_err(|error| error.to_string())?;
    let mut committed_memory = false;
    loop {
        if cancellation.is_cancelled() {
            return Ok(committed_memory);
        }
        let Some(job) = queue.claim_next().map_err(|error| error.to_string())? else {
            return Ok(committed_memory);
        };
        let transcript = vault
            .load_transcript(&job.session_id)
            .map_err(String::from)?;
        if !extraction::assistant_source_is_complete(&transcript, &job) {
            let _ = queue.abandon(&job.id, "source turn is no longer complete");
            continue;
        }
        let mut request = extraction::build_extraction_request(&character, &transcript, &job);
        let generation = generation::load(&vault).map_err(|error| error.to_string())?;
        let resolved = generation::resolve_extraction(&provider, &generation)
            .map_err(|error| error.to_string())?;
        request.provider_id = provider.id.clone();
        request.model = resolved.chat_model;
        request.temperature = resolved.temperature;
        request.max_tokens = resolved.max_tokens;
        let events = match transport
            .stream_chat(&provider, &request, cancellation.child_token())
            .await
        {
            Ok(events) => events,
            Err(error) => {
                if cancellation.is_cancelled() {
                    let _ = queue.defer(&job.id, "deferred for foreground conversation");
                    return Ok(committed_memory);
                }
                let message = error.to_string();
                let _ = queue.fail(&job.id, &message);
                continue;
            }
        };
        let mut output = String::new();
        let mut completed = false;
        let mut failed = None;
        for event in events {
            match event {
                providers::ChatStreamEvent::Delta { text } => output.push_str(&text),
                providers::ChatStreamEvent::Completed { .. } => completed = true,
                providers::ChatStreamEvent::Cancelled => {
                    failed = Some("extraction request cancelled".to_owned())
                }
                providers::ChatStreamEvent::Failed { message } => failed = Some(message),
                providers::ChatStreamEvent::Started { .. } => {}
            }
        }
        if !completed || failed.is_some() {
            if cancellation.is_cancelled() {
                let _ = queue.defer(&job.id, "deferred for foreground conversation");
                return Ok(committed_memory);
            }
            let message =
                failed.unwrap_or_else(|| "extraction provider ended without completion".to_owned());
            let _ = queue.fail(&job.id, &message);
            continue;
        }
        match queue.accept_model_output(&job.id, &output) {
            Ok(proposals) if !cancellation.is_cancelled() => {
                let mut memories = match memory::MemoryStore::open(&vault, &character.id) {
                    Ok(store) => store,
                    Err(error) => {
                        eprintln!(
                            "memory store unavailable after extraction; leaving proposals in inbox: {error}"
                        );
                        continue;
                    }
                };
                let mut service = reconciliation::ReconciliationService::new(&queue, &mut memories);
                for proposal in proposals {
                    let origin = extraction::effective_origin(&proposal.candidate, &transcript);
                    if origin != proposal.candidate.origin {
                        if let Err(error) = queue.reclassify_proposal(&proposal.id, origin.clone())
                        {
                            eprintln!(
                                "failed to reclassify extraction proposal {}: {error}",
                                proposal.id
                            );
                        }
                    }
                    if !extraction::is_auto_writable(&origin) {
                        continue;
                    }
                    match service.auto_commit(&proposal.id) {
                        Ok(reconciliation::CommitResult::Committed { .. }) => {
                            committed_memory = true;
                        }
                        Ok(_) => {}
                        Err(error) => {
                            eprintln!(
                                "auto-commit failed for proposal {}; leaving in inbox: {error}",
                                proposal.id
                            );
                        }
                    }
                }
            }
            _ => {}
        }
        tokio::task::yield_now().await;
    }
}

async fn process_embedding_rebuild(
    vault_root: String,
    provider: providers::ProviderConfig,
    character: storage::CharacterDefinition,
    cancellation: CancellationToken,
    transport: Arc<dyn conversation::ChatTransport>,
) -> Result<(), String> {
    tokio::task::yield_now().await;
    if cancellation.is_cancelled() {
        return Ok(());
    }
    let Some(model) = provider
        .embedding_model
        .as_deref()
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .map(str::to_owned)
    else {
        return Ok(());
    };
    let (vault, _) = open_character_vault(vault_root, &character.id)?;
    let mut store =
        memory::MemoryStore::open(&vault, &character.id).map_err(|error| error.to_string())?;
    let memories = store.list().map_err(|error| error.to_string())?;
    let chunks = embeddings::chunks_from_memories(&memories);
    let stored = embeddings::EmbeddingIndex::stored_space(&vault, &character.id)
        .map_err(|error| error.to_string())?;
    let matching_space = stored.filter(|space| {
        space.provider_id == provider.id
            && space.model == model
            && space.chunking_version == embeddings::CHUNKING_VERSION
            && space.index_version == embeddings::EMBEDDING_INDEX_VERSION
    });
    if cancellation.is_cancelled() {
        return Ok(());
    }
    if let Some(space) = matching_space {
        let index = embeddings::EmbeddingIndex::open(&vault, &character.id, space)
            .map_err(|error| error.to_string())?;
        if !index
            .needs_rebuild(&chunks)
            .map_err(|error| error.to_string())?
        {
            return Ok(());
        }
        index.begin_rebuild().map_err(|error| error.to_string())?;
        if cancellation.is_cancelled() {
            return Ok(());
        }
        let vectors = embed_chunks(
            transport.as_ref(),
            &provider,
            &model,
            &chunks,
            &cancellation,
        )
        .await?;
        if cancellation.is_cancelled() {
            return Ok(());
        }
        if vectors.len() != chunks.len() {
            return Err(format!(
                "embedding provider returned {} vectors for {} chunks",
                vectors.len(),
                chunks.len()
            ));
        }
        for (chunk, vector) in chunks.iter().zip(&vectors) {
            index
                .upsert(chunk, vector)
                .map_err(|error| error.to_string())?;
            tokio::task::yield_now().await;
            if cancellation.is_cancelled() {
                return Ok(());
            }
        }
        index.finish_rebuild().map_err(|error| error.to_string())?;
        tokio::task::yield_now().await;
        return Ok(());
    }
    if chunks.is_empty() {
        return Ok(());
    }
    let vectors = embed_chunks(
        transport.as_ref(),
        &provider,
        &model,
        &chunks,
        &cancellation,
    )
    .await?;
    if cancellation.is_cancelled() {
        return Ok(());
    }
    if vectors.len() != chunks.len() {
        return Err(format!(
            "embedding provider returned {} vectors for {} chunks",
            vectors.len(),
            chunks.len()
        ));
    }
    let dimensions = vectors.first().map(Vec::len).unwrap_or(0);
    if dimensions == 0 {
        return Err("embedding provider returned empty vectors".into());
    }
    let space = embeddings::EmbeddingSpace::new(&provider.id, model, dimensions);
    let index = embeddings::EmbeddingIndex::open(&vault, &character.id, space)
        .map_err(|error| error.to_string())?;
    index.begin_rebuild().map_err(|error| error.to_string())?;
    if cancellation.is_cancelled() {
        return Ok(());
    }
    for (chunk, vector) in chunks.iter().zip(&vectors) {
        index
            .upsert(chunk, vector)
            .map_err(|error| error.to_string())?;
        tokio::task::yield_now().await;
        if cancellation.is_cancelled() {
            return Ok(());
        }
    }
    index.finish_rebuild().map_err(|error| error.to_string())?;
    tokio::task::yield_now().await;
    Ok(())
}

async fn embed_chunks(
    transport: &dyn conversation::ChatTransport,
    provider: &providers::ProviderConfig,
    model: &str,
    chunks: &[embeddings::EmbeddingChunk],
    cancellation: &CancellationToken,
) -> Result<Vec<Vec<f32>>, String> {
    if chunks.is_empty() {
        return Ok(Vec::new());
    }
    let request = providers::EmbeddingRequest {
        provider_id: provider.id.clone(),
        model: model.to_owned(),
        input: chunks.iter().map(|chunk| chunk.content.clone()).collect(),
    };
    tokio::select! {
        _ = cancellation.cancelled() => Ok(Vec::new()),
        response = transport.embed(provider, &request) => response
            .map(|response| response.vectors)
            .map_err(|error| error.to_string()),
    }
}

#[tauri::command]
async fn initiative_send(
    app: AppHandle,
    state: tauri::State<'_, RuntimeState>,
    vault_root: String,
    mut request: initiative::InitiativeRequest,
    current_generation: u64,
    latest_user_activity_at: Option<i64>,
) -> Result<conversation::InitiativeOutcome, String> {
    let (vault, canonical_character) = open_character_vault(vault_root, &request.character.id)?;
    request.character = canonical_character;
    request.application_prompt = application_prompt_text(&app);
    let key = format!("{}:{}", request.character.id, request.session_id);
    let notification_character = request.character.clone();
    let (runtime_generation, cancellation) = state.begin(&key);
    let model_guard = state.model_gate.lock().await;
    let result = conversation::ConversationService::with_provider_client()
        .send_initiative(
            &vault,
            request,
            current_generation.max(runtime_generation),
            latest_user_activity_at,
            cancellation,
        )
        .await
        .map_err(String::from);
    drop(model_guard);
    state.finish(&key, runtime_generation);
    if let Ok(outcome) = &result {
        if matches!(
            outcome.status,
            conversation::InitiativeDeliveryStatus::Delivered
        ) {
            let _ = app.emit("initiative-delivered", outcome);
            let notifications_enabled =
                initiative::InitiativeStore::open(&vault, &notification_character.id)
                    .and_then(|store| store.settings())
                    .map(|settings| settings.notifications_enabled)
                    .unwrap_or(false);
            if notifications_enabled {
                if let Some(turn) = outcome
                    .initiative_turn
                    .as_ref()
                    .or(outcome.assistant_turn.as_ref())
                {
                    app.notification()
                        .builder()
                        .title(format!("{} reached out", notification_character.name))
                        .body(&turn.content)
                        .show()
                        .map_err(|error| error.to_string())?;
                }
            }
        }
    }
    result
}

#[tauri::command]
fn conversation_cancel(
    state: tauri::State<'_, RuntimeState>,
    character_id: String,
    session_id: String,
) -> bool {
    state.cancel(&format!("{}:{}", character_id, session_id))
}

#[tauri::command]
fn vault_export_pack(
    vault_root: String,
    destination: String,
) -> Result<portability::PackManifest, String> {
    let vault = storage::Vault::open(vault_root).map_err(String::from)?;
    portability::export_pack(&vault, destination).map_err(String::from)
}

#[tauri::command]
fn vault_import_pack(
    pack_root: String,
    destination_vault: String,
    replace_existing: bool,
) -> Result<portability::ImportResult, String> {
    portability::import_pack(pack_root, destination_vault, replace_existing).map_err(String::from)
}

#[tauri::command]
fn memory_browse(
    vault_root: String,
    character_id: String,
) -> Result<Vec<storage::MemoryRecord>, String> {
    let (vault, _) = open_character_vault(vault_root, &character_id)?;
    let mut store = memory::MemoryStore::open(&vault, &character_id).map_err(String::from)?;
    store.list().map_err(String::from)
}

#[tauri::command]
fn memory_search(
    vault_root: String,
    character_id: String,
    query: String,
    limit: usize,
) -> Result<Vec<storage::MemoryRecord>, String> {
    let (vault, _) = open_character_vault(vault_root, &character_id)?;
    let mut store = memory::MemoryStore::open(&vault, &character_id).map_err(String::from)?;
    store.search_records(&query, limit).map_err(String::from)
}

#[tauri::command]
fn memory_source_path(
    vault_root: String,
    character_id: String,
    source_path: String,
) -> Result<String, String> {
    let (vault, _) = open_character_vault(vault_root, &character_id)?;
    let relative = std::path::Path::new(&source_path);
    let starts_with_memories = matches!(
        relative.components().next(),
        Some(std::path::Component::Normal(component)) if component == "memories"
    );
    if !starts_with_memories || relative.extension().and_then(|value| value.to_str()) != Some("md")
    {
        return Err("memory source must be a Markdown file inside memories/".into());
    }
    let path = vault.resolve_relative(relative).map_err(String::from)?;
    if !path.is_file() {
        return Err(format!("memory source does not exist: {source_path}"));
    }
    std::fs::canonicalize(path)
        .map_err(|error| error.to_string())
        .map(|path| path.to_string_lossy().into_owned())
}

#[tauri::command]
fn memory_upsert(
    vault_root: String,
    character_id: String,
    memory_record: storage::MemoryRecord,
    original_memory_type: Option<storage::MemoryType>,
    original_id: Option<String>,
) -> Result<(), String> {
    let (vault, _) = open_character_vault(vault_root, &character_id)?;
    let mut store = memory::MemoryStore::open(&vault, &character_id).map_err(String::from)?;
    match (original_memory_type, original_id) {
        (Some(old_type), Some(old_id)) => {
            let old_path = vault
                .memory_path(&old_type, &old_id)
                .map_err(String::from)?;
            if !old_path.exists() {
                return Err(format!("original memory does not exist: {old_id}"));
            }
            if old_type == memory_record.memory_type && old_id == memory_record.id {
                return store.update(&memory_record).map_err(String::from);
            }
            return store
                .rename(&old_type, &old_id, &memory_record)
                .map_err(String::from);
        }
        (None, None) => {}
        _ => return Err("original memory type and id must be supplied together".into()),
    }
    let path = vault
        .memory_path(&memory_record.memory_type, &memory_record.id)
        .map_err(String::from)?;
    if path.exists() {
        store.update(&memory_record).map_err(String::from)
    } else {
        store.create(&memory_record).map_err(String::from)
    }
}

#[tauri::command]
fn memory_delete(
    vault_root: String,
    character_id: String,
    memory_type: storage::MemoryType,
    id: String,
) -> Result<(), String> {
    let (vault, _) = open_character_vault(vault_root, &character_id)?;
    let mut store = memory::MemoryStore::open(&vault, &character_id).map_err(String::from)?;
    let deleted = vault.load_memory(&memory_type, &id).map_err(String::from)?;
    store.delete(&memory_type, &id).map_err(String::from)?;
    let queue = extraction::ExtractionQueue::open(&vault, &character_id)
        .map_err(|error| error.to_string())?;
    queue
        .suppress_memory(&deleted, "user deleted memory")
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn memory_commit_proposal(
    vault_root: String,
    character_id: String,
    proposal_id: String,
) -> Result<reconciliation::CommitResult, String> {
    let (vault, _) = open_character_vault(vault_root, &character_id)?;
    let queue = extraction::ExtractionQueue::open(&vault, &character_id)
        .map_err(|error| error.to_string())?;
    let mut store = memory::MemoryStore::open(&vault, &character_id).map_err(String::from)?;
    reconciliation::ReconciliationService::new(&queue, &mut store)
        .commit_accepted(&proposal_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn memory_review_queue(
    vault_root: String,
    character_id: String,
) -> Result<Vec<extraction::MemoryProposal>, String> {
    let (vault, _) = open_character_vault(vault_root, &character_id)?;
    let queue = extraction::ExtractionQueue::open(&vault, &character_id)
        .map_err(|error| error.to_string())?;
    queue.pending_proposals().map_err(|error| error.to_string())
}

#[tauri::command]
fn memory_edit_proposal(
    vault_root: String,
    character_id: String,
    proposal_id: String,
    body: String,
    confidence: f32,
) -> Result<extraction::MemoryProposal, String> {
    let (vault, _) = open_character_vault(vault_root, &character_id)?;
    let queue = extraction::ExtractionQueue::open(&vault, &character_id)
        .map_err(|error| error.to_string())?;
    queue
        .edit_proposal(&proposal_id, &body, confidence)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn memory_accept_proposal(
    vault_root: String,
    character_id: String,
    proposal_id: String,
) -> Result<(), String> {
    let (vault, _) = open_character_vault(vault_root, &character_id)?;
    let queue = extraction::ExtractionQueue::open(&vault, &character_id)
        .map_err(|error| error.to_string())?;
    queue
        .mark_proposal(&proposal_id, extraction::ProposalStatus::Accepted)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn memory_reject_proposal(
    vault_root: String,
    character_id: String,
    proposal_id: String,
) -> Result<(), String> {
    let (vault, _) = open_character_vault(vault_root, &character_id)?;
    let queue = extraction::ExtractionQueue::open(&vault, &character_id)
        .map_err(|error| error.to_string())?;
    let mut store = memory::MemoryStore::open(&vault, &character_id).map_err(String::from)?;
    reconciliation::ReconciliationService::new(&queue, &mut store)
        .reject(&proposal_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn memory_repair_index(vault_root: String, character_id: String) -> Result<(), String> {
    let (vault, _) = open_character_vault(vault_root, &character_id)?;
    let mut store = memory::MemoryStore::open(&vault, &character_id).map_err(String::from)?;
    store.rebuild().map_err(String::from)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(RuntimeState::default())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                hide_window_instead_of_closing(
                    || api.prevent_close(),
                    || {
                        let _ = window.hide();
                    },
                );
            }
        })
        .setup(|app| {
            let show = MenuItemBuilder::with_id("show", "Show tz-chatter").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit tz-chatter").build(app)?;
            let menu = MenuBuilder::new(app).items(&[&show, &quit]).build()?;
            TrayIconBuilder::new()
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(
                        event,
                        TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        }
                    ) {
                        if let Some(window) = tray.app_handle().get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            provider_capabilities,
            validate_provider_config,
            load_provider_settings,
            save_provider_settings,
            provider_health,
            provider_discover,
            provider_embed,
            initiative_snapshot,
            initiative_save_settings,
            initiative_scheduler_start,
            initiative_scheduler_stop,
            initiative_record_user_activity,
            initiative_record_sent,
            initiative_record_ignored,
            initiative_record_resume,
            character_library_list,
            character_library_add,
            character_library_create,
            character_library_remove,
            character_load,
            character_save,
            character_portrait_load,
            character_portrait_set,
            character_portrait_clear,
            generation_load,
            generation_save,
            application_prompt_load,
            application_prompt_save,
            persona_load,
            persona_save,
            scene_settings_load,
            scene_settings_save,
            locals_list,
            local_load,
            local_save,
            local_delete,
            conversation_send,
            conversation_resume,
            conversation_list_sessions,
            conversation_search_sessions,
            conversation_rename_session,
            conversation_set_session_archived,
            conversation_open_session,
            conversation_start_session,
            conversation_set_session_local,
            conversation_retry,
            conversation_regenerate,
            conversation_continue,
            conversation_edit_last_user,
            conversation_cancel,
            initiative_send,
            vault_export_pack,
            vault_import_pack,
            memory_browse,
            memory_search,
            memory_source_path,
            memory_upsert,
            memory_delete,
            memory_review_queue,
            memory_edit_proposal,
            memory_accept_proposal,
            memory_commit_proposal,
            memory_reject_proposal,
            memory_repair_index
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::{
        hide_window_instead_of_closing, memory_source_path, memory_upsert, open_character_vault,
        process_embedding_rebuild, process_extraction_queue, RuntimeState,
    };
    use crate::conversation::ChatTransport;
    use crate::extraction::{ProposalOrigin, ProposalStatus};
    use crate::memory::MemoryStore;
    use crate::providers::{
        ChatStreamEvent, EmbeddingResponse, ProviderConfig, ProviderError, ProviderKind,
    };
    use crate::storage::{
        CharacterDefinition, MemoryRecord, MemoryType, TranscriptDocument, TranscriptTurn,
        TurnRole, TurnStatus, Vault,
    };
    use async_trait::async_trait;
    use std::fs;
    use std::sync::{Arc, Mutex};
    use tokio_util::sync::CancellationToken;

    #[test]
    fn replacing_session_work_cancels_previous_generation() {
        let state = RuntimeState::default();
        let (first_generation, first) = state.begin("lyra:session");
        let (second_generation, second) = state.begin("lyra:session");
        assert_eq!(first_generation, 1);
        assert_eq!(second_generation, 2);
        assert!(first.is_cancelled());
        assert!(!second.is_cancelled());

        state.finish("lyra:session", first_generation);
        assert!(!second.is_cancelled());
        assert!(state.cancel("lyra:session"));
        assert!(second.is_cancelled());
        let (third_generation, third) = state.begin("lyra:session");
        assert_eq!(third_generation, 3);
        assert!(!third.is_cancelled());
    }

    #[test]
    fn scheduler_replacement_and_child_work_are_cancellable() {
        let state = RuntimeState::default();
        let (first_generation, first) = state.start_scheduler("initiative:lyra:session");
        let (second_generation, second) = state.start_scheduler("initiative:lyra:session");
        assert_eq!(first_generation, 1);
        assert_eq!(second_generation, 2);
        assert!(first.is_cancelled());
        assert!(!second.is_cancelled());

        let (work_generation, work) = state
            .try_begin_child("lyra:session", &second)
            .expect("scheduler should be able to claim idle work");
        assert_eq!(work_generation, 1);
        assert!(state.try_begin_child("lyra:session", &second).is_none());
        state.finish("lyra:session", work_generation);
        assert!(state.stop_scheduler("initiative:lyra:session"));
        assert!(second.is_cancelled());
        assert!(work.is_cancelled());
        assert!(!state.stop_scheduler("initiative:lyra:session"));
    }

    #[test]
    fn foreground_work_cancels_background_model_work() {
        let state = RuntimeState::default();
        let background = state
            .begin_background("extraction:lyra")
            .expect("background work should start while idle");
        let (_, foreground) = state.begin("lyra:session");
        assert!(background.is_cancelled());
        assert!(!foreground.is_cancelled());
        assert!(state.begin_background("extraction:lyra").is_none());
    }

    #[test]
    fn extraction_and_embedding_background_keys_are_unique_per_kind() {
        let state = RuntimeState::default();
        let extraction = state
            .begin_background("extraction:lyra")
            .expect("extraction work should start while idle");
        let embedding = state
            .begin_background("embedding:lyra")
            .expect("embedding work should start beside extraction");
        assert!(!extraction.is_cancelled());
        assert!(!embedding.is_cancelled());
        assert!(state.begin_background("extraction:lyra").is_none());
        assert!(state.begin_background("embedding:lyra").is_none());
        let (_, foreground) = state.begin("lyra:session");
        assert!(extraction.is_cancelled());
        assert!(embedding.is_cancelled());
        assert!(!foreground.is_cancelled());
    }

    #[test]
    fn character_scoped_commands_reject_a_mismatched_vault() {
        let root = std::env::temp_dir().join(format!(
            "tz-chatter-command-vault-{}",
            crate::storage::new_stable_id()
        ));
        let vault = Vault::create(&root).unwrap();
        vault
            .save_character(&CharacterDefinition::new("lyra", "Lyra", "Stay grounded."))
            .unwrap();
        assert!(open_character_vault(&root, "lyra").is_ok());
        assert!(open_character_vault(&root, "nova")
            .unwrap_err()
            .contains("does not match"));
        assert!(open_character_vault(&root, "")
            .unwrap_err()
            .contains("load a character vault"));
        let missing = std::env::temp_dir().join(format!(
            "tz-chatter-missing-vault-{}",
            crate::storage::new_stable_id()
        ));
        assert!(open_character_vault(&missing, "lyra").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn close_requested_hides_the_window_instead_of_exiting() {
        let mut prevented = false;
        let mut hidden = false;
        hide_window_instead_of_closing(|| prevented = true, || hidden = true);
        assert!(prevented);
        assert!(hidden);
    }

    #[test]
    fn memory_upsert_moves_an_existing_record_without_leaving_a_duplicate() {
        let root = std::env::temp_dir().join(format!(
            "tz-chatter-command-memory-{}",
            crate::storage::new_stable_id()
        ));
        let vault = Vault::create(&root).unwrap();
        vault
            .save_character(&CharacterDefinition::new("lyra", "Lyra", "Stay grounded."))
            .unwrap();
        let original = MemoryRecord::new("friend", MemoryType::Semantic, "Mina likes tea.");
        memory_upsert(
            root.to_string_lossy().into_owned(),
            "lyra".into(),
            original.clone(),
            None,
            None,
        )
        .unwrap();
        let mut moved = original;
        moved.memory_type = MemoryType::People;
        moved.body = "Mina likes green tea.".into();
        memory_upsert(
            root.to_string_lossy().into_owned(),
            "lyra".into(),
            moved.clone(),
            Some(MemoryType::Semantic),
            Some("friend".into()),
        )
        .unwrap();

        assert!(!vault
            .memory_path(&MemoryType::Semantic, "friend")
            .unwrap()
            .exists());
        assert_eq!(
            vault.load_memory(&MemoryType::People, "friend").unwrap(),
            moved
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn memory_source_navigation_is_limited_to_existing_markdown_memories() {
        let root = std::env::temp_dir().join(format!(
            "tz-chatter-command-source-{}",
            crate::storage::new_stable_id()
        ));
        let vault = Vault::create(&root).unwrap();
        vault
            .save_character(&CharacterDefinition::new("lyra", "Lyra", "Stay grounded."))
            .unwrap();
        vault
            .save_memory(&MemoryRecord::new(
                "fact",
                MemoryType::Semantic,
                "Mina likes tea.",
            ))
            .unwrap();
        let root_string = root.to_string_lossy().into_owned();

        let source = memory_source_path(
            root_string.clone(),
            "lyra".into(),
            "memories/semantic/fact.md".into(),
        )
        .unwrap();
        assert_eq!(
            std::path::PathBuf::from(source),
            fs::canonicalize(vault.memory_path(&MemoryType::Semantic, "fact").unwrap()).unwrap()
        );
        assert!(memory_source_path(root_string, "lyra".into(), "../character.md".into(),).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    struct FakeExtractionTransport {
        responses: Mutex<Vec<Result<Vec<ChatStreamEvent>, ProviderError>>>,
    }

    #[async_trait]
    impl ChatTransport for FakeExtractionTransport {
        async fn stream_chat(
            &self,
            _config: &ProviderConfig,
            _request: &crate::providers::ChatRequest,
            _cancellation: CancellationToken,
        ) -> Result<Vec<ChatStreamEvent>, ProviderError> {
            self.responses.lock().unwrap().remove(0)
        }
    }

    struct CapturingExtractionTransport {
        responses: Mutex<Vec<Result<Vec<ChatStreamEvent>, ProviderError>>>,
        requests: Mutex<Vec<crate::providers::ChatRequest>>,
    }

    #[async_trait]
    impl ChatTransport for CapturingExtractionTransport {
        async fn stream_chat(
            &self,
            _config: &ProviderConfig,
            request: &crate::providers::ChatRequest,
            _cancellation: CancellationToken,
        ) -> Result<Vec<ChatStreamEvent>, ProviderError> {
            self.requests.lock().unwrap().push(request.clone());
            self.responses.lock().unwrap().remove(0)
        }
    }

    fn extraction_output(source: &str) -> String {
        extraction_output_with(source, "user_stated", "User likes quiet cafes.")
    }

    fn extraction_output_with(source: &str, origin: &str, body: &str) -> String {
        format!(
            r#"{{"schema_version":1,"proposals":[{{"memory_type":"semantic","body":"{body}","source_turn_ids":["{source}"],"evidence":[{{"turn_id":"{source}","quote":"quiet cafes"}}],"confidence":0.8,"origin":"{origin}"}}]}}"#
        )
    }

    #[tokio::test]
    async fn extraction_worker_drains_pending_jobs_after_resume() {
        let root = std::env::temp_dir().join(format!(
            "tz-chatter-command-extract-{}",
            crate::storage::new_stable_id()
        ));
        let vault = Vault::create(&root).unwrap();
        let character = CharacterDefinition::new("lyra", "Lyra", "Stay grounded.");
        vault.save_character(&character).unwrap();
        let mut transcript = TranscriptDocument::new("session-1", "lyra");
        transcript.turns.push(TranscriptTurn {
            id: "user-1".into(),
            timestamp: "1".into(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: "I like quiet cafes.".into(),
        });
        transcript.turns.push(TranscriptTurn {
            id: "assistant-1".into(),
            timestamp: "2".into(),
            role: TurnRole::Assistant,
            status: TurnStatus::Complete,
            content: "I will remember that.".into(),
        });
        transcript.turns.push(TranscriptTurn {
            id: "user-2".into(),
            timestamp: "3".into(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: "I also like tea.".into(),
        });
        transcript.turns.push(TranscriptTurn {
            id: "assistant-2".into(),
            timestamp: "4".into(),
            role: TurnRole::Assistant,
            status: TurnStatus::Complete,
            content: "Noted.".into(),
        });
        vault.save_transcript(&transcript).unwrap();
        let mut queue = crate::extraction::ExtractionQueue::open(&vault, "lyra").unwrap();
        queue
            .enqueue_transcript(&transcript, "assistant-1", 100)
            .unwrap();
        queue
            .enqueue_transcript(&transcript, "assistant-2", 100)
            .unwrap();
        drop(queue);

        let transport: Arc<dyn ChatTransport> = Arc::new(FakeExtractionTransport {
            responses: Mutex::new(vec![
                Ok(vec![
                    ChatStreamEvent::Delta {
                        text: extraction_output("user-1"),
                    },
                    ChatStreamEvent::Completed {
                        finish_reason: Some("stop".into()),
                    },
                ]),
                Ok(vec![
                    ChatStreamEvent::Delta {
                        text: extraction_output_with(
                            "user-2",
                            "inferred",
                            "The user might prefer mornings.",
                        ),
                    },
                    ChatStreamEvent::Completed {
                        finish_reason: Some("stop".into()),
                    },
                ]),
            ]),
        });
        process_extraction_queue(
            root.to_string_lossy().into_owned(),
            ProviderConfig {
                id: "fake".into(),
                kind: ProviderKind::Ollama,
                endpoint: "http://127.0.0.1:11434".into(),
                chat_model: "fake-model".into(),
                embedding_model: None,
                bearer_token: None,
            },
            character,
            CancellationToken::new(),
            transport,
        )
        .await
        .unwrap();

        let mut queue = crate::extraction::ExtractionQueue::open(&vault, "lyra").unwrap();
        let mut memories = MemoryStore::open(&vault, "lyra").unwrap();
        let listed = memories.list().unwrap();
        assert!(listed
            .iter()
            .any(|memory| memory.body.contains("User likes quiet cafes.")));
        assert!(!listed
            .iter()
            .any(|memory| memory.body.contains("The user might prefer mornings.")));
        let pending = queue.pending_proposals().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].candidate.origin, ProposalOrigin::Inferred);
        assert!(pending[0]
            .candidate
            .body
            .contains("The user might prefer mornings."));
        assert!(queue.claim_next().unwrap().is_none());
        drop(memories);
        drop(queue);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn extraction_worker_abandons_superseded_source_without_writing() {
        let root = std::env::temp_dir().join(format!(
            "tz-chatter-command-extract-superseded-{}",
            crate::storage::new_stable_id()
        ));
        let vault = Vault::create(&root).unwrap();
        let character = CharacterDefinition::new("lyra", "Lyra", "Stay grounded.");
        vault.save_character(&character).unwrap();
        let mut transcript = TranscriptDocument::new("session-1", "lyra");
        transcript.turns.push(TranscriptTurn {
            id: "user-1".into(),
            timestamp: "1".into(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: "I like quiet cafes.".into(),
        });
        transcript.turns.push(TranscriptTurn {
            id: "assistant-1".into(),
            timestamp: "2".into(),
            role: TurnRole::Assistant,
            status: TurnStatus::Complete,
            content: "I will remember that.".into(),
        });
        vault.save_transcript(&transcript).unwrap();
        let mut queue = crate::extraction::ExtractionQueue::open(&vault, "lyra").unwrap();
        queue
            .enqueue_transcript(&transcript, "assistant-1", 100)
            .unwrap();
        drop(queue);
        transcript.turns[1].status = TurnStatus::Superseded;
        vault.save_transcript(&transcript).unwrap();

        let transport: Arc<dyn ChatTransport> = Arc::new(FakeExtractionTransport {
            responses: Mutex::new(Vec::new()),
        });
        process_extraction_queue(
            root.to_string_lossy().into_owned(),
            extraction_provider(),
            character,
            CancellationToken::new(),
            transport,
        )
        .await
        .unwrap();

        let mut memories = MemoryStore::open(&vault, "lyra").unwrap();
        assert!(memories.list().unwrap().is_empty());
        let mut queue = crate::extraction::ExtractionQueue::open(&vault, "lyra").unwrap();
        assert!(queue.claim_next().unwrap().is_none());
        drop(memories);
        drop(queue);
        fs::remove_dir_all(root).unwrap();
    }

    fn sabotage_semantic_writes(vault: &Vault) {
        let semantic = vault.root().join("memories").join("semantic");
        if semantic.is_dir() {
            fs::remove_dir_all(&semantic).unwrap();
        }
        if let Some(parent) = semantic.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&semantic, b"not-a-directory").unwrap();
    }

    fn extraction_provider() -> ProviderConfig {
        ProviderConfig {
            id: "fake".into(),
            kind: ProviderKind::Ollama,
            endpoint: "http://127.0.0.1:11434".into(),
            chat_model: "fake-model".into(),
            embedding_model: None,
            bearer_token: None,
        }
    }

    #[tokio::test]
    async fn extraction_uses_character_model_not_sampling() {
        let root = std::env::temp_dir().join(format!(
            "tz-chatter-extract-generation-{}",
            crate::storage::new_stable_id()
        ));
        let vault = Vault::create(&root).unwrap();
        let character = CharacterDefinition::new("lyra", "Lyra", "You are Lyra.");
        vault.save_character(&character).unwrap();
        crate::generation::save(
            &vault,
            &crate::generation::CharacterGeneration {
                schema_version: 1,
                chat_model: Some("lyra-voice".into()),
                temperature: Some(0.9),
                max_tokens: Some(64),
            },
        )
        .unwrap();
        let mut transcript = TranscriptDocument::new("session-1", "lyra");
        transcript.turns.push(TranscriptTurn {
            id: "user-1".into(),
            timestamp: "1".into(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: "I like quiet cafes.".into(),
        });
        transcript.turns.push(TranscriptTurn {
            id: "assistant-1".into(),
            timestamp: "2".into(),
            role: TurnRole::Assistant,
            status: TurnStatus::Complete,
            content: "I will remember that.".into(),
        });
        vault.save_transcript(&transcript).unwrap();
        let mut queue = crate::extraction::ExtractionQueue::open(&vault, "lyra").unwrap();
        queue
            .enqueue_transcript(&transcript, "assistant-1", 100)
            .unwrap();
        drop(queue);

        let transport = Arc::new(CapturingExtractionTransport {
            responses: Mutex::new(vec![Ok(vec![
                ChatStreamEvent::Delta {
                    text: extraction_output("user-1"),
                },
                ChatStreamEvent::Completed {
                    finish_reason: Some("stop".into()),
                },
            ])]),
            requests: Mutex::new(Vec::new()),
        });
        process_extraction_queue(
            root.to_string_lossy().into_owned(),
            extraction_provider(),
            character,
            CancellationToken::new(),
            transport.clone(),
        )
        .await
        .unwrap();
        let captured = transport.requests.lock().unwrap()[0].clone();
        assert_eq!(captured.model, "lyra-voice");
        assert_eq!(
            captured.temperature,
            Some(crate::generation::EXTRACTION_TEMPERATURE)
        );
        assert_eq!(
            captured.max_tokens,
            Some(crate::generation::EXTRACTION_MAX_TOKENS)
        );
        fs::remove_dir_all(root).ok();
    }

    fn embedding_provider() -> ProviderConfig {
        ProviderConfig {
            id: "fake".into(),
            kind: ProviderKind::Ollama,
            endpoint: "http://127.0.0.1:11434".into(),
            chat_model: "fake-model".into(),
            embedding_model: Some("fake-embed".into()),
            bearer_token: None,
        }
    }

    #[tokio::test]
    async fn extraction_worker_continues_after_auto_commit_error() {
        let root = std::env::temp_dir().join(format!(
            "tz-chatter-command-extract-err-{}",
            crate::storage::new_stable_id()
        ));
        let vault = Vault::create(&root).unwrap();
        let character = CharacterDefinition::new("lyra", "Lyra", "Stay grounded.");
        vault.save_character(&character).unwrap();
        let mut transcript = TranscriptDocument::new("session-1", "lyra");
        transcript.turns.push(TranscriptTurn {
            id: "user-1".into(),
            timestamp: "1".into(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: "I like quiet cafes.".into(),
        });
        transcript.turns.push(TranscriptTurn {
            id: "assistant-1".into(),
            timestamp: "2".into(),
            role: TurnRole::Assistant,
            status: TurnStatus::Complete,
            content: "I will remember that.".into(),
        });
        transcript.turns.push(TranscriptTurn {
            id: "user-2".into(),
            timestamp: "3".into(),
            role: TurnRole::User,
            status: TurnStatus::Complete,
            content: "I also like tea.".into(),
        });
        transcript.turns.push(TranscriptTurn {
            id: "assistant-2".into(),
            timestamp: "4".into(),
            role: TurnRole::Assistant,
            status: TurnStatus::Complete,
            content: "Noted.".into(),
        });
        vault.save_transcript(&transcript).unwrap();
        let mut queue = crate::extraction::ExtractionQueue::open(&vault, "lyra").unwrap();
        queue
            .enqueue_transcript(&transcript, "assistant-1", 100)
            .unwrap();
        queue
            .enqueue_transcript(&transcript, "assistant-2", 100)
            .unwrap();
        drop(queue);
        sabotage_semantic_writes(&vault);

        let transport: Arc<dyn ChatTransport> = Arc::new(FakeExtractionTransport {
            responses: Mutex::new(vec![
                Ok(vec![
                    ChatStreamEvent::Delta {
                        text: extraction_output("user-1"),
                    },
                    ChatStreamEvent::Completed {
                        finish_reason: Some("stop".into()),
                    },
                ]),
                Ok(vec![
                    ChatStreamEvent::Delta {
                        text: extraction_output_with(
                            "user-2",
                            "inferred",
                            "The user might prefer mornings.",
                        ),
                    },
                    ChatStreamEvent::Completed {
                        finish_reason: Some("stop".into()),
                    },
                ]),
            ]),
        });
        let result = process_extraction_queue(
            root.to_string_lossy().into_owned(),
            extraction_provider(),
            character,
            CancellationToken::new(),
            transport,
        )
        .await;
        assert!(
            result.is_ok(),
            "drain must continue after auto-commit error: {result:?}"
        );

        let mut queue = crate::extraction::ExtractionQueue::open(&vault, "lyra").unwrap();
        let pending = queue.pending_proposals().unwrap();
        assert_eq!(pending.len(), 2);
        assert!(pending
            .iter()
            .all(|proposal| proposal.status == ProposalStatus::NeedsReview));
        assert!(pending
            .iter()
            .any(|proposal| proposal.candidate.body.contains("User likes quiet cafes.")));
        assert!(pending.iter().any(|proposal| proposal
            .candidate
            .body
            .contains("The user might prefer mornings.")));
        assert!(queue.claim_next().unwrap().is_none());
        drop(queue);
        fs::remove_dir_all(root).unwrap();
    }

    struct FakeEmbedTransport {
        hang: bool,
        started: tokio::sync::Notify,
        vector: Vec<f32>,
    }

    #[async_trait]
    impl ChatTransport for FakeEmbedTransport {
        async fn stream_chat(
            &self,
            config: &ProviderConfig,
            _request: &crate::providers::ChatRequest,
            _cancellation: CancellationToken,
        ) -> Result<Vec<ChatStreamEvent>, ProviderError> {
            Err(ProviderError::UnsupportedCapability {
                capability: "chat".into(),
                provider: config.kind.clone(),
            })
        }

        async fn embed(
            &self,
            _config: &ProviderConfig,
            request: &crate::providers::EmbeddingRequest,
        ) -> Result<EmbeddingResponse, ProviderError> {
            self.started.notify_waiters();
            if self.hang {
                std::future::pending::<()>().await;
            }
            Ok(EmbeddingResponse {
                model: request.model.clone(),
                dimensions: self.vector.len(),
                vectors: request.input.iter().map(|_| self.vector.clone()).collect(),
            })
        }
    }

    #[tokio::test]
    async fn embedding_rebuild_worker_clears_needs_rebuild_when_chunks_exist() {
        let root = std::env::temp_dir().join(format!(
            "tz-chatter-command-embed-ready-{}",
            crate::storage::new_stable_id()
        ));
        let vault = Vault::create(&root).unwrap();
        let character = CharacterDefinition::new("lyra", "Lyra", "Stay grounded.");
        vault.save_character(&character).unwrap();
        let mut store = MemoryStore::open(&vault, "lyra").unwrap();
        store
            .create(&MemoryRecord::new(
                "tea",
                MemoryType::Semantic,
                "Mina drinks tea.",
            ))
            .unwrap();
        let chunks = crate::embeddings::chunks_from_memories(&store.list().unwrap());
        drop(store);
        assert!(!chunks.is_empty());

        let transport: Arc<dyn ChatTransport> = Arc::new(FakeEmbedTransport {
            hang: false,
            started: tokio::sync::Notify::new(),
            vector: vec![1.0, 0.0],
        });
        process_embedding_rebuild(
            root.to_string_lossy().into_owned(),
            embedding_provider(),
            character,
            CancellationToken::new(),
            transport,
        )
        .await
        .unwrap();

        let space = crate::embeddings::EmbeddingIndex::stored_space(&vault, "lyra")
            .unwrap()
            .expect("embedding space after rebuild");
        let index = crate::embeddings::EmbeddingIndex::open(&vault, "lyra", space).unwrap();
        assert!(index.is_ready().unwrap());
        assert!(!index.needs_rebuild(&chunks).unwrap());
        drop(index);
        fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn embedding_rebuild_cancel_before_finish_leaves_index_not_ready() {
        let root = std::env::temp_dir().join(format!(
            "tz-chatter-command-embed-cancel-{}",
            crate::storage::new_stable_id()
        ));
        let vault = Vault::create(&root).unwrap();
        let character = CharacterDefinition::new("lyra", "Lyra", "Stay grounded.");
        vault.save_character(&character).unwrap();
        let mut store = MemoryStore::open(&vault, "lyra").unwrap();
        store
            .create(&MemoryRecord::new(
                "tea",
                MemoryType::Semantic,
                "Mina drinks tea.",
            ))
            .unwrap();
        let chunks = crate::embeddings::chunks_from_memories(&store.list().unwrap());
        drop(store);

        let space = crate::embeddings::EmbeddingSpace::new("fake", "fake-embed", 2);
        let index = crate::embeddings::EmbeddingIndex::open(&vault, "lyra", space.clone()).unwrap();
        index.begin_rebuild().unwrap();
        index.finish_rebuild().unwrap();
        assert!(index.needs_rebuild(&chunks).unwrap());
        drop(index);

        let transport = Arc::new(FakeEmbedTransport {
            hang: true,
            started: tokio::sync::Notify::new(),
            vector: vec![1.0, 0.0],
        });
        let started = transport.started.notified();
        let cancellation = CancellationToken::new();
        let job = tokio::spawn(process_embedding_rebuild(
            root.to_string_lossy().into_owned(),
            embedding_provider(),
            character,
            cancellation.clone(),
            transport.clone(),
        ));
        started.await;
        cancellation.cancel();
        job.await.unwrap().unwrap();

        let index = crate::embeddings::EmbeddingIndex::open(&vault, "lyra", space).unwrap();
        assert!(
            !index.is_ready().unwrap() || index.needs_rebuild(&chunks).unwrap(),
            "cancelled rebuild must not leave a ready matching index"
        );
        drop(index);
        fs::remove_dir_all(root).unwrap();
    }
}
