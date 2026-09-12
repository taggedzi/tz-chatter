use crate::{
    memory::MemoryStore,
    providers::ProviderConfig,
    storage::{CharacterDefinition, MemoryReviewStatus, MemoryType, Vault},
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::fmt;

pub const INITIATIVE_SCHEMA_VERSION: u32 = 1;
const SECONDS_PER_DAY: i64 = 86_400;
const MINUTES_PER_DAY: i64 = 1_440;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InitiativeSettings {
    pub schema_version: u32,
    pub enabled: bool,
    pub notifications_enabled: bool,
    pub min_inactive_seconds: i64,
    pub cooldown_seconds: i64,
    pub max_per_day: u32,
    pub max_ignored: u32,
    pub quiet_start_minute: u16,
    pub quiet_end_minute: u16,
}

impl Default for InitiativeSettings {
    fn default() -> Self {
        Self {
            schema_version: INITIATIVE_SCHEMA_VERSION,
            enabled: false,
            notifications_enabled: false,
            min_inactive_seconds: 15 * 60,
            cooldown_seconds: 30 * 60,
            max_per_day: 3,
            max_ignored: 3,
            quiet_start_minute: 22 * 60,
            quiet_end_minute: 7 * 60,
        }
    }
}

impl InitiativeSettings {
    pub fn validate(&self) -> Result<(), InitiativeError> {
        if self.schema_version != INITIATIVE_SCHEMA_VERSION {
            return Err(InitiativeError::Invalid(
                "unsupported initiative settings schema".into(),
            ));
        }
        if self.min_inactive_seconds < 0 || self.cooldown_seconds < 0 {
            return Err(InitiativeError::Invalid(
                "initiative durations cannot be negative".into(),
            ));
        }
        if self.max_per_day == 0 || self.max_ignored == 0 {
            return Err(InitiativeError::Invalid(
                "initiative caps must be greater than zero".into(),
            ));
        }
        if self.quiet_start_minute >= MINUTES_PER_DAY as u16
            || self.quiet_end_minute >= MINUTES_PER_DAY as u16
        {
            return Err(InitiativeError::Invalid(
                "quiet-hour minutes must be within one day".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InitiativeState {
    pub schema_version: u32,
    pub last_user_activity_at: Option<i64>,
    pub last_initiative_at: Option<i64>,
    pub day_key: i64,
    pub sent_today: u32,
    pub ignored_streak: u32,
    pub unanswered: bool,
    pub resume_suppressed_until: i64,
}

impl Default for InitiativeState {
    fn default() -> Self {
        Self {
            schema_version: INITIATIVE_SCHEMA_VERSION,
            last_user_activity_at: None,
            last_initiative_at: None,
            day_key: 0,
            sent_today: 0,
            ignored_streak: 0,
            unanswered: false,
            resume_suppressed_until: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InitiativeContext {
    pub active_character: bool,
    pub model_available: bool,
    pub resource_available: bool,
    pub utc_offset_minutes: i16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitiativeRequest {
    pub character: CharacterDefinition,
    pub provider: ProviderConfig,
    pub session_id: String,
    pub request_id: String,
    pub topic_context: String,
    pub generation: u64,
    pub started_at: i64,
    #[serde(default)]
    pub application_prompt: String,
}

pub fn select_open_topics(
    vault: &Vault,
    character_id: &str,
    limit: usize,
) -> Result<String, InitiativeError> {
    if limit == 0 {
        return Ok(String::new());
    }
    let mut store = MemoryStore::open(vault, character_id)
        .map_err(|error| InitiativeError::Storage(error.to_string()))?;
    let topics = store
        .list()
        .map_err(|error| InitiativeError::Storage(error.to_string()))?
        .into_iter()
        .filter(|memory| {
            memory.memory_type == MemoryType::OpenThreads
                && memory.review_status == MemoryReviewStatus::Accepted
        })
        .take(limit)
        .map(|memory| {
            let body = memory.body.trim();
            let bounded_body = body.chars().take(360).collect::<String>();
            let labels = memory.topics.join(", ");
            if labels.is_empty() {
                format!("- {}: {}", memory.id, bounded_body)
            } else {
                format!("- {} [{}]: {}", memory.id, labels, bounded_body)
            }
        })
        .collect::<Vec<_>>();
    Ok(topics.join("\n"))
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum InitiativeModelChoice {
    Silence,
    Message,
}

pub fn classify_initiative_response(content: &str) -> InitiativeModelChoice {
    let normalized = content.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized == "silence" || normalized == "<silence>" {
        InitiativeModelChoice::Silence
    } else {
        InitiativeModelChoice::Message
    }
}

pub fn render_initiative_event(topic_context: &str) -> String {
    let topic = if topic_context.trim().is_empty() {
        "There are no confirmed open topics. You may choose silence."
    } else {
        topic_context.trim()
    };
    format!("[INTERNAL INITIATIVE EVENT] You may choose SILENCE. If you speak, send one brief relevant message grounded only in the character definition, conversation, and archival context. Open topics and context: {topic}")
}

pub fn initiative_is_stale(
    request: &InitiativeRequest,
    current_generation: u64,
    latest_user_activity_at: Option<i64>,
) -> bool {
    request.generation != current_generation
        || latest_user_activity_at.is_some_and(|activity| activity > request.started_at)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EligibilityReason {
    Disabled,
    NoActiveCharacter,
    ModelUnavailable,
    ResourceUnavailable,
    NoUserActivity,
    UserRecentlyActive,
    QuietHours,
    Cooldown,
    FrequencyCap,
    UnansweredInitiative,
    IgnoredBackoff,
    ResumeSuppression,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EligibilityDecision {
    pub eligible: bool,
    pub reasons: Vec<EligibilityReason>,
    pub effective_cooldown_seconds: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InitiativeSnapshot {
    pub settings: InitiativeSettings,
    pub state: InitiativeState,
    pub decision: EligibilityDecision,
}

#[derive(Debug)]
pub enum InitiativeError {
    Storage(String),
    Invalid(String),
}

impl fmt::Display for InitiativeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Storage(message) => write!(formatter, "initiative storage error: {message}"),
            Self::Invalid(message) => write!(formatter, "invalid initiative state: {message}"),
        }
    }
}

impl std::error::Error for InitiativeError {}

impl From<rusqlite::Error> for InitiativeError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Storage(error.to_string())
    }
}

impl From<InitiativeError> for String {
    fn from(error: InitiativeError) -> Self {
        error.to_string()
    }
}

pub struct InitiativeStore {
    connection: Connection,
    character_id: String,
}

impl InitiativeStore {
    pub fn open(vault: &Vault, character_id: &str) -> Result<Self, InitiativeError> {
        if character_id.trim().is_empty() {
            return Err(InitiativeError::Invalid(
                "character_id cannot be empty".into(),
            ));
        }
        let state_dir = vault.root().join(".tz-chatter");
        std::fs::create_dir_all(&state_dir)
            .map_err(|error| InitiativeError::Storage(error.to_string()))?;
        let connection = Connection::open(state_dir.join("state.sqlite3"))?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS initiative_settings (
                 character_id TEXT PRIMARY KEY,
                 schema_version INTEGER NOT NULL,
                 enabled INTEGER NOT NULL,
                 notifications_enabled INTEGER NOT NULL DEFAULT 0,
                 min_inactive_seconds INTEGER NOT NULL,
                 cooldown_seconds INTEGER NOT NULL,
                 max_per_day INTEGER NOT NULL,
                 max_ignored INTEGER NOT NULL,
                 quiet_start_minute INTEGER NOT NULL,
                 quiet_end_minute INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS initiative_state (
                 character_id TEXT PRIMARY KEY,
                 schema_version INTEGER NOT NULL,
                 last_user_activity_at INTEGER,
                 last_initiative_at INTEGER,
                 day_key INTEGER NOT NULL,
                 sent_today INTEGER NOT NULL,
                 ignored_streak INTEGER NOT NULL,
                 unanswered INTEGER NOT NULL,
                 resume_suppressed_until INTEGER NOT NULL
             );",
        )?;
        let _ = connection.execute(
            "ALTER TABLE initiative_settings ADD COLUMN notifications_enabled INTEGER NOT NULL DEFAULT 0",
            [],
        );
        Ok(Self {
            connection,
            character_id: character_id.into(),
        })
    }

    pub fn settings(&self) -> Result<InitiativeSettings, InitiativeError> {
        let result = self.connection.query_row(
            "SELECT schema_version, enabled, notifications_enabled, min_inactive_seconds, cooldown_seconds,
                    max_per_day, max_ignored, quiet_start_minute, quiet_end_minute
             FROM initiative_settings WHERE character_id = ?1",
            params![self.character_id],
            |row| {
                Ok(InitiativeSettings {
                    schema_version: row.get::<_, i64>(0)? as u32,
                    enabled: row.get::<_, i64>(1)? != 0,
                    notifications_enabled: row.get::<_, i64>(2)? != 0,
                    min_inactive_seconds: row.get(3)?,
                    cooldown_seconds: row.get(4)?,
                    max_per_day: row.get::<_, i64>(5)? as u32,
                    max_ignored: row.get::<_, i64>(6)? as u32,
                    quiet_start_minute: row.get::<_, i64>(7)? as u16,
                    quiet_end_minute: row.get::<_, i64>(8)? as u16,
                })
            },
        );
        match result {
            Ok(settings) => {
                settings.validate()?;
                Ok(settings)
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(InitiativeSettings::default()),
            Err(error) => Err(error.into()),
        }
    }

    pub fn save_settings(&self, settings: &InitiativeSettings) -> Result<(), InitiativeError> {
        settings.validate()?;
        self.connection.execute(
            "INSERT INTO initiative_settings
             (character_id, schema_version, enabled, notifications_enabled, min_inactive_seconds,
              cooldown_seconds, max_per_day, max_ignored, quiet_start_minute, quiet_end_minute)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(character_id) DO UPDATE SET
              schema_version = excluded.schema_version,
              enabled = excluded.enabled,
              notifications_enabled = excluded.notifications_enabled,
              min_inactive_seconds = excluded.min_inactive_seconds,
              cooldown_seconds = excluded.cooldown_seconds,
              max_per_day = excluded.max_per_day,
              max_ignored = excluded.max_ignored,
              quiet_start_minute = excluded.quiet_start_minute,
              quiet_end_minute = excluded.quiet_end_minute",
            params![
                self.character_id,
                settings.schema_version,
                settings.enabled as i64,
                settings.notifications_enabled as i64,
                settings.min_inactive_seconds,
                settings.cooldown_seconds,
                settings.max_per_day,
                settings.max_ignored,
                settings.quiet_start_minute,
                settings.quiet_end_minute
            ],
        )?;
        Ok(())
    }

    pub fn state(&self) -> Result<InitiativeState, InitiativeError> {
        let result = self.connection.query_row(
            "SELECT schema_version, last_user_activity_at, last_initiative_at, day_key,
                    sent_today, ignored_streak, unanswered, resume_suppressed_until
             FROM initiative_state WHERE character_id = ?1",
            params![self.character_id],
            |row| {
                Ok(InitiativeState {
                    schema_version: row.get::<_, i64>(0)? as u32,
                    last_user_activity_at: row.get(1)?,
                    last_initiative_at: row.get(2)?,
                    day_key: row.get(3)?,
                    sent_today: row.get::<_, i64>(4)? as u32,
                    ignored_streak: row.get::<_, i64>(5)? as u32,
                    unanswered: row.get::<_, i64>(6)? != 0,
                    resume_suppressed_until: row.get(7)?,
                })
            },
        );
        match result {
            Ok(state) if state.schema_version == INITIATIVE_SCHEMA_VERSION => Ok(state),
            Ok(_) => Err(InitiativeError::Invalid(
                "unsupported initiative state schema".into(),
            )),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(InitiativeState::default()),
            Err(error) => Err(error.into()),
        }
    }

    fn save_state(&self, state: &InitiativeState) -> Result<(), InitiativeError> {
        self.connection.execute(
            "INSERT INTO initiative_state
             (character_id, schema_version, last_user_activity_at, last_initiative_at, day_key,
              sent_today, ignored_streak, unanswered, resume_suppressed_until)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(character_id) DO UPDATE SET
              schema_version = excluded.schema_version,
              last_user_activity_at = excluded.last_user_activity_at,
              last_initiative_at = excluded.last_initiative_at,
              day_key = excluded.day_key,
              sent_today = excluded.sent_today,
              ignored_streak = excluded.ignored_streak,
              unanswered = excluded.unanswered,
              resume_suppressed_until = excluded.resume_suppressed_until",
            params![
                self.character_id,
                state.schema_version,
                state.last_user_activity_at,
                state.last_initiative_at,
                state.day_key,
                state.sent_today,
                state.ignored_streak,
                state.unanswered as i64,
                state.resume_suppressed_until
            ],
        )?;
        Ok(())
    }

    pub fn record_user_activity(&self, now: i64) -> Result<InitiativeState, InitiativeError> {
        let mut state = self.state()?;
        state.last_user_activity_at = Some(now);
        state.unanswered = false;
        state.ignored_streak = 0;
        self.save_state(&state)?;
        Ok(state)
    }

    pub fn record_initiative_sent(&self, now: i64) -> Result<InitiativeState, InitiativeError> {
        let mut state = self.state()?;
        state.last_initiative_at = Some(now);
        state.sent_today = state.sent_today.saturating_add(1);
        state.unanswered = true;
        self.save_state(&state)?;
        Ok(state)
    }

    pub fn record_initiative_ignored(&self, _now: i64) -> Result<InitiativeState, InitiativeError> {
        let mut state = self.state()?;
        state.unanswered = false;
        state.ignored_streak = state.ignored_streak.saturating_add(1);
        self.save_state(&state)?;
        Ok(state)
    }

    pub fn record_initiative_silenced(&self, now: i64) -> Result<InitiativeState, InitiativeError> {
        let mut state = self.state()?;
        state.last_initiative_at = Some(now);
        state.unanswered = false;
        self.save_state(&state)?;
        Ok(state)
    }

    pub fn record_resume(&self, now: i64) -> Result<InitiativeState, InitiativeError> {
        let settings = self.settings()?;
        let mut state = self.state()?;
        state.resume_suppressed_until = now.saturating_add(settings.min_inactive_seconds);
        self.save_state(&state)?;
        Ok(state)
    }

    pub fn evaluate(
        &self,
        now: i64,
        context: InitiativeContext,
    ) -> Result<InitiativeSnapshot, InitiativeError> {
        let settings = self.settings()?;
        let mut state = self.state()?;
        let decision = evaluate_eligibility(&settings, &mut state, now, &context);
        self.save_state(&state)?;
        Ok(InitiativeSnapshot {
            settings,
            state,
            decision,
        })
    }
}

pub fn evaluate_eligibility(
    settings: &InitiativeSettings,
    state: &mut InitiativeState,
    now: i64,
    context: &InitiativeContext,
) -> EligibilityDecision {
    let mut reasons = Vec::new();
    rotate_day(state, now, context.utc_offset_minutes);
    if !settings.enabled {
        reasons.push(EligibilityReason::Disabled);
    }
    if !context.active_character {
        reasons.push(EligibilityReason::NoActiveCharacter);
    }
    if !context.model_available {
        reasons.push(EligibilityReason::ModelUnavailable);
    }
    if !context.resource_available {
        reasons.push(EligibilityReason::ResourceUnavailable);
    }
    let inactive = state
        .last_user_activity_at
        .map(|activity| now.saturating_sub(activity))
        .unwrap_or_default();
    if state.last_user_activity_at.is_none() {
        reasons.push(EligibilityReason::NoUserActivity);
    } else if inactive < settings.min_inactive_seconds {
        reasons.push(EligibilityReason::UserRecentlyActive);
    }
    if quiet_hours(
        now,
        settings.quiet_start_minute,
        settings.quiet_end_minute,
        context.utc_offset_minutes,
    ) {
        reasons.push(EligibilityReason::QuietHours);
    }
    if now < state.resume_suppressed_until {
        reasons.push(EligibilityReason::ResumeSuppression);
    }
    if state.sent_today >= settings.max_per_day {
        reasons.push(EligibilityReason::FrequencyCap);
    }
    if state.unanswered {
        reasons.push(EligibilityReason::UnansweredInitiative);
    }
    let effective_cooldown_seconds = effective_cooldown(settings, state.ignored_streak);
    if state
        .last_initiative_at
        .is_some_and(|sent| now.saturating_sub(sent) < effective_cooldown_seconds)
    {
        reasons.push(EligibilityReason::Cooldown);
    }
    if state.ignored_streak >= settings.max_ignored {
        reasons.push(EligibilityReason::IgnoredBackoff);
    }
    EligibilityDecision {
        eligible: reasons.is_empty(),
        reasons,
        effective_cooldown_seconds,
    }
}

fn rotate_day(state: &mut InitiativeState, now: i64, utc_offset_minutes: i16) {
    let day = day_key(now, utc_offset_minutes);
    if state.day_key != day {
        state.day_key = day;
        state.sent_today = 0;
    }
}

fn day_key(now: i64, utc_offset_minutes: i16) -> i64 {
    now.saturating_add(i64::from(utc_offset_minutes) * 60)
        .div_euclid(SECONDS_PER_DAY)
}

fn effective_cooldown(settings: &InitiativeSettings, ignored_streak: u32) -> i64 {
    let multiplier = 1_i64.checked_shl(ignored_streak.min(6)).unwrap_or(64);
    settings
        .cooldown_seconds
        .saturating_mul(multiplier)
        .min(7 * SECONDS_PER_DAY)
}

fn quiet_hours(now: i64, start: u16, end: u16, utc_offset_minutes: i16) -> bool {
    if start == end {
        return false;
    }
    let minute = now
        .saturating_add(i64::from(utc_offset_minutes) * 60)
        .rem_euclid(SECONDS_PER_DAY)
        / 60;
    if start < end {
        minute >= i64::from(start) && minute < i64::from(end)
    } else {
        minute >= i64::from(start) || minute < i64::from(end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{MemoryRecord, MemoryReviewStatus, MemoryType, Vault};
    use std::{fs, path::PathBuf};

    fn root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "tz-chatter-initiative-{}",
            crate::storage::new_stable_id()
        ))
    }

    fn context() -> InitiativeContext {
        InitiativeContext {
            active_character: true,
            model_available: true,
            resource_available: true,
            utc_offset_minutes: 0,
        }
    }

    fn eligible_settings() -> InitiativeSettings {
        InitiativeSettings {
            enabled: true,
            min_inactive_seconds: 60,
            cooldown_seconds: 300,
            max_per_day: 2,
            max_ignored: 2,
            quiet_start_minute: 22 * 60,
            quiet_end_minute: 7 * 60,
            ..InitiativeSettings::default()
        }
    }

    #[test]
    fn quiet_hour_boundaries_are_deterministic() {
        let settings = eligible_settings();
        let day = 20_000_i64;
        let mut state = InitiativeState {
            last_user_activity_at: Some(day * SECONDS_PER_DAY),
            ..InitiativeState::default()
        };
        let before = day * SECONDS_PER_DAY + 21 * 3_600 + 59 * 60;
        let at_start = day * SECONDS_PER_DAY + 22 * 3_600;
        assert!(evaluate_eligibility(&settings, &mut state, before, &context()).eligible);
        let decision = evaluate_eligibility(&settings, &mut state, at_start, &context());
        assert!(decision.reasons.contains(&EligibilityReason::QuietHours));
        assert!(!decision.eligible);
    }

    #[test]
    fn disabled_settings_and_frequency_caps_never_schedule() {
        let mut settings = eligible_settings();
        settings.enabled = false;
        let mut state = InitiativeState {
            last_user_activity_at: Some(1),
            ..InitiativeState::default()
        };
        let decision = evaluate_eligibility(&settings, &mut state, 10_000, &context());
        assert!(!decision.eligible);
        assert!(decision.reasons.contains(&EligibilityReason::Disabled));

        settings.enabled = true;
        state.sent_today = settings.max_per_day;
        state.day_key = day_key(10_000, 0);
        let decision = evaluate_eligibility(&settings, &mut state, 10_000, &context());
        assert!(decision.reasons.contains(&EligibilityReason::FrequencyCap));
    }

    #[test]
    fn restart_persists_cooldown_unanswered_and_ignored_backoff() {
        let root = root();
        let vault = Vault::create(&root).unwrap();
        let store = InitiativeStore::open(&vault, "lyra").unwrap();
        let mut settings = eligible_settings();
        settings.notifications_enabled = true;
        store.save_settings(&settings).unwrap();
        store.record_user_activity(100).unwrap();
        store.record_initiative_sent(1_000).unwrap();
        drop(store);

        let reopened = InitiativeStore::open(&vault, "lyra").unwrap();
        assert!(reopened.settings().unwrap().notifications_enabled);
        let blocked = reopened.evaluate(1_100, context()).unwrap();
        assert!(blocked
            .decision
            .reasons
            .contains(&EligibilityReason::UnansweredInitiative));
        reopened.record_initiative_ignored(2_000).unwrap();
        reopened.record_initiative_ignored(3_000).unwrap();
        let backed_off = reopened.evaluate(4_000, context()).unwrap();
        assert!(backed_off
            .decision
            .reasons
            .contains(&EligibilityReason::IgnoredBackoff));
        let silenced = reopened.record_initiative_silenced(5_000).unwrap();
        assert_eq!(silenced.last_initiative_at, Some(5_000));
        assert!(!silenced.unanswered);
        drop(reopened);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn resume_suppression_prevents_sleep_catch_up_burst() {
        let settings = eligible_settings();
        let mut state = InitiativeState {
            last_user_activity_at: Some(1),
            ..InitiativeState::default()
        };
        let now = 50_000;
        state.resume_suppressed_until = now + settings.min_inactive_seconds;
        let during_resume = evaluate_eligibility(&settings, &mut state, now + 1, &context());
        assert!(!during_resume.eligible);
        assert!(during_resume
            .reasons
            .contains(&EligibilityReason::ResumeSuppression));
        let after_suppression = evaluate_eligibility(
            &settings,
            &mut state,
            now + settings.min_inactive_seconds,
            &context(),
        );
        assert!(after_suppression.eligible, "{after_suppression:?}");
    }

    #[test]
    fn open_topic_selection_is_bounded_and_excludes_unreviewed_records() {
        let root = root();
        let vault = Vault::create(&root).unwrap();
        let mut store = MemoryStore::open(&vault, "lyra").unwrap();
        let mut accepted = MemoryRecord::new(
            "garden",
            MemoryType::OpenThreads,
            "Follow up on the garden project and the spring planting plan.",
        );
        accepted.created_at = "1".into();
        accepted.updated_at = "2".into();
        accepted.topics = vec!["garden".into()];
        store.create(&accepted).unwrap();

        let mut excluded = MemoryRecord::new(
            "private",
            MemoryType::OpenThreads,
            "This excluded topic must never be offered to initiative.",
        );
        excluded.created_at = "1".into();
        excluded.updated_at = "3".into();
        excluded.review_status = MemoryReviewStatus::Excluded;
        store.create(&excluded).unwrap();

        let context = select_open_topics(&vault, "lyra", 4).unwrap();
        assert!(context.contains("garden"));
        assert!(!context.contains("private"));
        assert!(select_open_topics(&vault, "lyra", 0).unwrap().is_empty());
        drop(store);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn quiet_hours_apply_the_supplied_local_offset() {
        let settings = eligible_settings();
        let day = 20_000_i64;
        let mut state = InitiativeState {
            last_user_activity_at: Some(day * SECONDS_PER_DAY),
            ..InitiativeState::default()
        };
        let utc_time = day * SECONDS_PER_DAY + 17 * 3_600;
        let local_context = InitiativeContext {
            utc_offset_minutes: 5 * 60,
            ..context()
        };
        let decision = evaluate_eligibility(&settings, &mut state, utc_time, &local_context);
        assert!(decision.reasons.contains(&EligibilityReason::QuietHours));
    }
}
