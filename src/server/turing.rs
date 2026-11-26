use super::state::ArenaMode;
mod chat;
mod content;
use axum::body::Bytes;
pub(super) use chat::{ChatSettings, DEFAULT_URL};
use chrono::{DateTime, Utc};
pub(super) use content::Sample;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    env, io,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::{fs, sync::Mutex};
use uuid::Uuid;

#[derive(Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
pub(super) enum Answer {
    AI,
    Human,
}
impl Answer {
    pub fn label(self) -> &'static str {
        match self {
            Self::AI => "AI",
            Self::Human => "Human",
        }
    }
}

#[derive(Clone, Deserialize, Serialize)]
struct Vote {
    login: String,
    choice: Answer,
}

#[derive(Clone, Deserialize, Serialize)]
struct ResultRecord {
    id: Uuid,
    title: String,
    answer: Answer,
    closed: i64,
    channel: String,
    votes: BTreeMap<String, Vote>,
}

#[derive(Clone)]
pub(super) struct Turing {
    content: PathBuf,
    results: PathBuf,
    http: reqwest::Client,
    live: Arc<Mutex<Option<Round>>>,
}

impl Default for Turing {
    fn default() -> Self {
        Self {
            content: env::var_os("ARENA_TURING_CONTENT")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("../../2026/summer/events/turing-test/content")
                }),
            results: env::var_os("ARENA_TURING_RESULTS")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    dirs::data_local_dir()
                        .unwrap_or_else(env::temp_dir)
                        .join("ai-arena/turing")
                }),
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(4))
                .build()
                .expect("HTTP client"),
            live: Default::default(),
        }
    }
}

#[derive(Deserialize)]
struct Events {
    events: Vec<Event>,
}
#[derive(Deserialize)]
struct Event {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    source: String,
    channel: Option<String>,
    timestamp: DateTime<Utc>,
    payload: serde_json::Value,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RoundState {
    Live,
    Saving,
    Revealed,
    Unscored,
}

struct Round {
    mode: ArenaMode,
    record: ResultRecord,
    media: Bytes,
    mime: &'static str,
    started: i64,
    opens: i64,
    cursor: Option<String>,
    message_ids: HashSet<String>,
    warning: String,
    state: RoundState,
}

impl Round {
    fn active(&self) -> bool {
        matches!(self.state, RoundState::Live | RoundState::Saving)
    }

    async fn reveal(&mut self, results: &Path) -> io::Result<()> {
        if self.mode == ArenaMode::Real {
            let id = self.record.id;
            let tmp = results.join(format!("{id}.tmp"));
            fs::write(&tmp, serde_json::to_vec(&self.record)?).await?;
            fs::rename(tmp, results.join(format!("{id}.json"))).await?;
        }
        self.state = RoundState::Revealed;
        self.warning.clear();
        Ok(())
    }

    fn ingest(&mut self, events: &[Event]) {
        let start = match &self.cursor {
            None => 0,
            Some(cursor) => match events.iter().position(|e| e.id == *cursor) {
                Some(index) => index + 1,
                None => {
                    self.state = RoundState::Unscored;
                    self.warning = "Chat history has a gap. This round was not scored.".into();
                    return;
                }
            },
        };
        for e in &events[start..] {
            let at = e.timestamp.timestamp_millis();
            if e.channel.as_deref() != Some(&self.record.channel)
                || at < self.started
                || at >= self.record.closed
            {
                continue;
            }
            if e.kind == "twitch.status" && e.payload["connected"].is_boolean() {
                self.state = RoundState::Unscored;
                self.warning =
                    "The Twitch chat connection changed. This round was not scored.".into();
                return;
            }
            if at < self.opens || e.source != "twitch" || e.kind != "chat.message" {
                continue;
            }
            let (Some(id), Some(login), Some(text)) = (
                e.payload["user_id"].as_str(),
                e.payload["user_login"].as_str(),
                e.payload["text"].as_str(),
            ) else {
                continue;
            };
            if id.is_empty() || login.is_empty() {
                continue;
            }
            let mut words = text.split_whitespace();
            if !words
                .next()
                .is_some_and(|w| w.eq_ignore_ascii_case("!vote"))
            {
                continue;
            }
            let choice = match words.next().map(str::to_ascii_lowercase).as_deref() {
                Some("ai") => Answer::AI,
                Some("human") => Answer::Human,
                _ => continue,
            };
            if let Some(msg) = e.payload["message_id"].as_str()
                && !self.message_ids.insert(msg.to_string())
            {
                continue;
            }
            if words.next().is_none() {
                self.record.votes.insert(
                    id.into(),
                    Vote {
                        login: login.into(),
                        choice,
                    },
                );
            }
        }
        self.cursor = events.last().map(|e| e.id.clone());
    }
}

#[derive(Default)]
pub(super) struct Stage {
    pub id: String,
    pub title: String,
    pub phase: &'static str,
    pub seconds: i64,
    pub text: String,
    pub kind: &'static str,
    pub channel: String,
    pub warning: String,
    pub answer: String,
    pub turnout: usize,
    pub test_round: bool,
    pub human: usize,
    pub ai: usize,
    pub verdict: String,
    pub active: bool,
}

impl Turing {
    pub async fn samples(&self) -> Result<Vec<Sample>, String> {
        let root = self.content.clone();
        tokio::task::spawn_blocking(move || content::library(&root))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| {
                format!("Cannot load the Turing Test library. Check ARENA_TURING_CONTENT ({e}).")
            })
    }

    async fn events(&self, bridge: &str) -> Result<Vec<Event>, reqwest::Error> {
        Ok(self
            .http
            .get(format!("{bridge}/api/events?limit=500"))
            .send()
            .await?
            .error_for_status()?
            .json::<Events>()
            .await?
            .events)
    }

    async fn channel(&self, bridge: &str) -> Result<String, String> {
        #[derive(Deserialize)]
        struct Health {
            configured: Option<bool>,
            twitch: Connection,
            channel: String,
        }
        #[derive(Deserialize)]
        struct Connection {
            connected: bool,
        }
        let health = async {
            self.http
                .get(format!("{bridge}/api/health"))
                .send()
                .await?
                .error_for_status()?
                .json::<Health>()
                .await
        }
        .await
        .map_err(|_| format!("Cannot reach Twitch Interactions at {bridge}. Check the service URL and that the service is running."))?;
        if health.configured == Some(false) {
            return Err("Twitch Interactions needs credentials. Set TC_USERNAME and TC_PASSWORD in ~/.twitch_chat on the service host, then restart Twitch Interactions.".into());
        }
        if !health.twitch.connected || health.channel.is_empty() {
            return Err("Twitch Interactions is online, but chat is disconnected. Check the channel and login in the service dashboard.".into());
        }
        Ok(health.channel)
    }

    async fn connect_chat(&self, bridge: &str) -> Result<(String, Vec<Event>), String> {
        let channel = self.channel(bridge).await?;
        let events = self.events(bridge).await.map_err(|_| {
            format!("Twitch Interactions at {bridge} did not return a readable event feed.")
        })?;
        Ok((channel, events))
    }

    pub async fn test_chat(&self, settings: &ChatSettings) -> Result<String, String> {
        let (channel, _) = self.connect_chat(&settings.url).await?;
        Ok(format!(
            "Connected to #{channel}. Chat events are available."
        ))
    }

    pub async fn start(
        &self,
        artifact: &str,
        review: u32,
        voting: u32,
        mode: ArenaMode,
    ) -> Result<(), String> {
        if !(1..=1800).contains(&review) || !(1..=1800).contains(&voting) {
            return Err("Use 1–1800 seconds for each phase.".into());
        }
        let sample = self
            .samples()
            .await?
            .into_iter()
            .find(|a| a.id == artifact)
            .ok_or("Choose an available sample.")?;
        let media: Bytes = fs::read(&sample.path)
            .await
            .map_err(|e| e.to_string())?
            .into();
        if sample.mime == "text/plain" && std::str::from_utf8(&media).is_err() {
            return Err("Text samples must be UTF-8.".into());
        }
        if mode == ArenaMode::Real {
            fs::create_dir_all(&self.results)
                .await
                .map_err(|e| format!("Cannot open the results directory: {e}"))?;
        }
        let bridge = ChatSettings::load().await?.url;
        let (channel, events) = self.connect_chat(&bridge).await?;
        let mut live = self.live.lock().await;
        if live.as_ref().is_some_and(Round::active) {
            return Err("A round is already in progress.".into());
        }
        let started = Utc::now().timestamp_millis();
        let opens = started + i64::from(review) * 1000;
        let id = Uuid::new_v4();
        *live = Some(Round {
            mode,
            record: ResultRecord {
                id,
                title: sample.title,
                answer: sample.answer,
                closed: opens + i64::from(voting) * 1000,
                channel,
                votes: Default::default(),
            },
            media,
            mime: sample.mime,
            started,
            opens,
            cursor: events.last().map(|e| e.id.clone()),
            message_ids: HashSet::new(),
            warning: String::new(),
            state: RoundState::Live,
        });
        tokio::spawn(self.clone().collect(id, bridge));
        Ok(())
    }

    async fn collect(self, id: Uuid, bridge: String) {
        loop {
            tokio::time::sleep(Duration::from_millis(650)).await;
            let state = self
                .live
                .lock()
                .await
                .as_ref()
                .filter(|r| r.record.id == id)
                .map(|r| r.state);
            let before = Utc::now().timestamp_millis();
            let update = match state {
                Some(RoundState::Live) => Some(self.events(&bridge).await),
                Some(RoundState::Saving) => None,
                _ => return,
            };
            let mut live = self.live.lock().await;
            let Some(round) = live.as_mut().filter(|r| r.record.id == id && r.active()) else {
                return;
            };
            match update {
                Some(Ok(events)) => {
                    round.warning.clear();
                    round.ingest(&events);
                    if !round.active() {
                        return;
                    }
                    if before < round.record.closed {
                        continue;
                    }
                    round.state = RoundState::Saving;
                }
                Some(Err(_)) => {
                    round.warning = "Reconnecting to Twitch Interactions…".into();
                    continue;
                }
                None => {}
            }
            if let Err(e) = round.reveal(&self.results).await {
                round.warning = format!("Could not save scores: {e}");
                continue;
            }
            return;
        }
    }

    pub async fn cancel(&self, id: Uuid) {
        if let Some(r) = self
            .live
            .lock()
            .await
            .as_mut()
            .filter(|r| r.record.id == id && r.active())
        {
            r.state = RoundState::Unscored;
            r.warning = "Round cancelled. Votes were not scored.".into();
        }
    }

    pub async fn media(&self, id: Uuid) -> Option<(&'static str, Bytes)> {
        self.live
            .lock()
            .await
            .as_ref()
            .filter(|r| r.record.id == id && r.mime != "text/plain")
            .map(|r| (r.mime, r.media.clone()))
    }

    pub async fn stage(&self) -> Stage {
        let live = self.live.lock().await;
        let Some(r) = live.as_ref() else {
            return Stage {
                phase: "Waiting for a round",
                ..Default::default()
            };
        };
        let now = Utc::now().timestamp_millis();
        let (phase, until) = if r.state == RoundState::Unscored {
            ("Unscored", now)
        } else if r.state == RoundState::Revealed {
            ("Reveal", now)
        } else if now < r.opens {
            ("Review", r.opens)
        } else if now < r.record.closed {
            ("Vote", r.record.closed)
        } else {
            ("Counting votes", now)
        };
        let mut s = Stage {
            id: r.record.id.to_string(),
            title: r.record.title.clone(),
            phase,
            seconds: (until - now + 999) / 1000,
            kind: r.mime.split('/').next().unwrap_or(""),
            channel: r.record.channel.clone(),
            warning: r.warning.clone(),
            active: r.active(),
            turnout: r.record.votes.len(),
            test_round: r.mode == ArenaMode::Test,
            ..Default::default()
        };
        if s.kind == "text" {
            s.text = String::from_utf8_lossy(&r.media).into_owned();
        }
        if r.state == RoundState::Revealed {
            s.answer = r.record.answer.label().into();
            s.ai = r
                .record
                .votes
                .values()
                .filter(|v| v.choice == Answer::AI)
                .count();
            s.human = r.record.votes.len() - s.ai;
            s.verdict = match (s.ai, s.human) {
                (0, 0) => "No votes".into(),
                (a, h) if a == h => "Chat tied".into(),
                (a, h) if (a > h) == (r.record.answer == Answer::AI) => {
                    "Chat guessed correctly".into()
                }
                _ => "Chat guessed incorrectly".into(),
            };
        }
        s
    }

    pub async fn scores(&self, lowest: bool) -> io::Result<Vec<Score>> {
        let mut files = match fs::read_dir(&self.results).await {
            Ok(files) => files,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(e),
        };
        let mut viewers: BTreeMap<String, Score> = BTreeMap::new();
        while let Some(file) = files.next_entry().await? {
            let path = file.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            let round: ResultRecord = serde_json::from_slice(&fs::read(path).await?)?;
            for (id, vote) in round.votes {
                let score = viewers.entry(id).or_default();
                if round.closed >= score.last {
                    score.login = vote.login;
                    score.last = round.closed;
                }
                score.total += 1;
                score.correct += u64::from(vote.choice == round.answer);
            }
        }
        let mut rows: Vec<_> = viewers.into_values().collect();
        rows.sort_by(|a, b| {
            let accuracy = (a.correct * b.total).cmp(&(b.correct * a.total));
            (if lowest { accuracy } else { accuracy.reverse() })
                .then(b.total.cmp(&a.total))
                .then(a.login.cmp(&b.login))
        });
        Ok(rows)
    }
}

#[derive(Default)]
pub(super) struct Score {
    pub login: String,
    pub total: u64,
    pub correct: u64,
    last: i64,
}
impl Score {
    pub fn accuracy(&self) -> String {
        format!("{:.1}%", 100.0 * self.correct as f64 / self.total as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round() -> Round {
        Round {
            mode: ArenaMode::Real,
            record: ResultRecord {
                id: Uuid::new_v4(),
                title: "Sample".into(),
                channel: "arena".into(),
                answer: Answer::Human,
                closed: 2000,
                votes: BTreeMap::new(),
            },
            started: 0,
            opens: 1000,
            state: RoundState::Live,
            mime: "text/plain",
            media: Bytes::new(),
            cursor: None,
            message_ids: HashSet::new(),
            warning: String::new(),
        }
    }
    fn vote(id: &str, time: i64, user: &str, choice: &str) -> Event {
        Event {
            id: id.into(),
            kind: "chat.message".into(),
            source: "twitch".into(),
            channel: Some("arena".into()),
            timestamp: DateTime::from_timestamp_millis(time).unwrap(),
            payload: serde_json::json!({"user_id":user,"user_login":user,"message_id":id,"text":choice}),
        }
    }

    #[test]
    fn only_authenticated_in_window_final_choices_count() {
        let mut r = round();
        let mut events = vec![
            vote("1", 999, "early", "!vote ai"),
            vote("2", 1000, "viewer", "!vote AI"),
            vote("3", 1500, "viewer", "!vote Human"),
        ];
        r.ingest(&events);
        events.push(vote("2", 1600, "viewer", "!vote AI"));
        events.push(vote("4", 1999, "second", "!VOTE ai"));
        events.push(vote("5", 2000, "late", "!vote ai"));
        let mut fake = vote("6", 1400, "test", "!vote ai");
        fake.source = "local".into();
        events.push(fake);
        let mut other = vote("7", 1600, "elsewhere", "!vote ai");
        other.channel = Some("another".into());
        events.push(other);
        r.ingest(&events);
        r.ingest(&events);
        assert_eq!(r.record.votes.len(), 2);
        assert!(r.record.votes["viewer"].choice == Answer::Human);
        assert!(r.record.votes["second"].choice == Answer::AI);
        r.ingest(&[vote("9", 1900, "other", "!vote ai")]);
        assert!(!r.active());
    }

    #[tokio::test]
    async fn test_votes_reveal_without_writing_results_or_changing_standings() {
        let tmp = tempfile::tempdir().unwrap();
        let turing = Turing {
            results: tmp.path().join("results"),
            ..Default::default()
        };
        let mut practice = round();
        practice.mode = ArenaMode::Test;
        practice.ingest(&[vote("1", 1400, "viewer", "!vote human")]);
        practice.reveal(&turing.results).await.unwrap();
        *turing.live.lock().await = Some(practice);
        let stage = turing.stage().await;
        assert_eq!((stage.phase, stage.human), ("Reveal", 1));
        assert!(!turing.results.exists());
        assert!(turing.scores(false).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn revealed_rounds_rank_stable_viewer_ids_across_restarts() {
        let dir = tempfile::tempdir().unwrap();
        for n in 0..2 {
            let mut r = round();
            let mut one = vote("1", 1200, "11", "!vote human");
            one.payload["user_login"] = if n == 0 { "old_name" } else { "new_name" }.into();
            r.ingest(&[one, vote("2", 1300, "22", "!vote ai")]);
            r.record.closed += n;
            fs::write(
                dir.path().join(format!("{}.json", r.record.id)),
                serde_json::to_vec(&r.record).unwrap(),
            )
            .await
            .unwrap();
        }
        let t = Turing {
            results: dir.path().into(),
            ..Default::default()
        };
        let rows = t.scores(false).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(
            (&*rows[0].login, rows[0].total, rows[0].correct),
            ("new_name", 2, 2)
        );
        assert_eq!(t.scores(true).await.unwrap()[0].correct, 0);
    }
}
