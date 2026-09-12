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

pub mod connections;
pub mod conversation;
pub mod embeddings;
pub mod extraction;
pub mod initiative;
pub mod memory;
pub mod portability;
pub mod prompt;
pub mod providers;
pub mod reconciliation;
pub mod retrieval;
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
async fn conversation_send(
    app: AppHandle,
    state: tauri::State<'_, RuntimeState>,
    vault_root: String,
    mut snapshot: conversation::RequestSnapshot,
    on_event: Channel<providers::ChatStreamEvent>,
) -> Result<conversation::ConversationOutcome, String> {
    let (vault, canonical_character) = open_character_vault(&vault_root, &snapshot.character.id)?;
    snapshot.character = canonical_character;
    let _ = initiative::InitiativeStore::open(&vault, &snapshot.character.id)
        .and_then(|store| store.record_user_activity(unix_now()));
    let key = format!("{}:{}", snapshot.character.id, snapshot.session_id);
    let extraction_provider = snapshot.provider.clone();
    let extraction_character = snapshot.character.clone();
    let (generation, cancellation) = state.begin(&key);
    let sink: connections::StreamSink = Arc::new(move |event| {
        let _ = on_event.send(event);
    });
    let model_guard = state.model_gate.lock().await;
    let result = conversation::ConversationService::with_provider_client()
        .send_streaming(&vault, snapshot, cancellation, Some(sink))
        .await
        .map_err(String::from);
    drop(model_guard);
    state.finish(&key, generation);
    if let Ok(outcome) = &result {
        if outcome.assistant_turn.status == storage::TurnStatus::Complete {
            schedule_extraction(
                app.clone(),
                &vault_root,
                extraction_provider,
                extraction_character,
            );
        }
    }
    result
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
    if let Ok(provider) = active_provider(&app) {
        schedule_extraction(app, &vault_root, provider, resumed.character.clone());
    }
    Ok(resumed)
}

#[tauri::command]
async fn conversation_retry(
    app: AppHandle,
    state: tauri::State<'_, RuntimeState>,
    vault_root: String,
    mut snapshot: conversation::RequestSnapshot,
    on_event: Channel<providers::ChatStreamEvent>,
) -> Result<conversation::ConversationOutcome, String> {
    let (vault, canonical_character) = open_character_vault(&vault_root, &snapshot.character.id)?;
    snapshot.character = canonical_character;
    let _ = initiative::InitiativeStore::open(&vault, &snapshot.character.id)
        .and_then(|store| store.record_user_activity(unix_now()));
    let key = format!("{}:{}", snapshot.character.id, snapshot.session_id);
    let extraction_provider = snapshot.provider.clone();
    let extraction_character = snapshot.character.clone();
    let (generation, cancellation) = state.begin(&key);
    let sink: connections::StreamSink = Arc::new(move |event| {
        let _ = on_event.send(event);
    });
    let model_guard = state.model_gate.lock().await;
    let result = conversation::ConversationService::with_provider_client()
        .retry_streaming(&vault, snapshot, cancellation, Some(sink))
        .await
        .map_err(String::from);
    drop(model_guard);
    state.finish(&key, generation);
    if let Ok(outcome) = &result {
        if outcome.assistant_turn.status == storage::TurnStatus::Complete {
            schedule_extraction(
                app.clone(),
                &vault_root,
                extraction_provider,
                extraction_character,
            );
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
        let result =
            process_extraction_queue(vault_root, provider, character, cancellation, transport)
                .await;
        drop(model_guard);
        runtime.finish_background(&key);
        if let Err(error) = result {
            eprintln!("background extraction did not complete: {error}");
        }
    });
}

async fn process_extraction_queue(
    vault_root: String,
    provider: providers::ProviderConfig,
    character: storage::CharacterDefinition,
    cancellation: CancellationToken,
    transport: Arc<dyn conversation::ChatTransport>,
) -> Result<(), String> {
    tokio::task::yield_now().await;
    let (vault, _) = open_character_vault(vault_root, &character.id)?;
    let mut queue = extraction::ExtractionQueue::open(&vault, &character.id)
        .map_err(|error| error.to_string())?;
    loop {
        if cancellation.is_cancelled() {
            return Ok(());
        }
        let Some(job) = queue.claim_next().map_err(|error| error.to_string())? else {
            return Ok(());
        };
        let transcript = vault
            .load_transcript(&job.session_id)
            .map_err(String::from)?;
        let mut request = extraction::build_extraction_request(&character, &transcript, &job);
        request.provider_id = provider.id.clone();
        request.model = provider.chat_model.clone();
        let events = match transport
            .stream_chat(&provider, &request, cancellation.child_token())
            .await
        {
            Ok(events) => events,
            Err(error) => {
                if cancellation.is_cancelled() {
                    let _ = queue.defer(&job.id, "deferred for foreground conversation");
                    return Ok(());
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
                return Ok(());
            }
            let message =
                failed.unwrap_or_else(|| "extraction provider ended without completion".to_owned());
            let _ = queue.fail(&job.id, &message);
            continue;
        }
        let _ = queue.accept_model_output(&job.id, &output);
        tokio::task::yield_now().await;
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
            conversation_send,
            conversation_resume,
            conversation_retry,
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
        process_extraction_queue, RuntimeState,
    };
    use crate::conversation::ChatTransport;
    use crate::providers::{ChatStreamEvent, ProviderConfig, ProviderError, ProviderKind};
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

    fn extraction_output(source: &str) -> String {
        format!(
            r#"{{"schema_version":1,"proposals":[{{"memory_type":"semantic","body":"User likes quiet cafes.","source_turn_ids":["{source}"],"evidence":[{{"turn_id":"{source}","quote":"quiet cafes"}}],"confidence":0.8,"origin":"user_stated"}}]}}"#
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
                        text: extraction_output("user-2"),
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
        assert_eq!(queue.pending_proposals().unwrap().len(), 2);
        assert!(queue.claim_next().unwrap().is_none());
        drop(queue);
        fs::remove_dir_all(root).unwrap();
    }
}
