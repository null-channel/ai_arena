mod cluster;
mod tasks;

use super::state::ArenaMode;
use crate::agent::{AIAgent, AgentError, MoveRequest};
use chrono::{DateTime, Utc};
use cluster::Cluster;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::HashMap, env, io, path::PathBuf, sync::Arc, time::Duration};
pub(super) use tasks::Task;
use tasks::{Action, action_schema};
use tokio::{
    fs,
    sync::RwLock,
    time::{Instant, sleep, timeout},
};
use uuid::Uuid;

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Case {
    pub task: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub agent: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub goal: String,
    pub status: String,
    pub action: String,
    pub evidence: String,
}
#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Report {
    pub id: Uuid,
    pub started: DateTime<Utc>,
    pub model: String,
    #[serde(default)]
    pub matrix: bool,
    pub cluster: String,
    pub mode: ArenaMode,
    pub running: bool,
    pub phase: String,
    pub error: String,
    pub cases: Vec<Case>,
}
impl Report {
    pub fn completed(&self) -> usize {
        self.cases
            .iter()
            .filter(|c| matches!(c.status.as_str(), "Passed" | "Failed" | "Error"))
            .count()
    }
    pub fn rows(&self) -> Vec<&[Case]> {
        self.cases.chunk_by(|a, b| a.agent == b.agent).collect()
    }
    pub fn kind(&self) -> &'static str {
        if self.matrix { "Matrix" } else { "Single" }
    }
    fn from_disk(bytes: &[u8]) -> io::Result<Self> {
        let mut report: Self = serde_json::from_slice(bytes)?;
        if report.running {
            report.running = false;
            report.phase = "Interrupted".into();
            for case in &mut report.cases {
                match case.status.as_str() {
                    "Running" => case.status = "Interrupted".into(),
                    "Queued" => case.status = "Skipped".into(),
                    _ => {}
                }
            }
        }
        Ok(report)
    }
    pub fn passed(&self) -> usize {
        self.cases.iter().filter(|c| c.status == "Passed").count()
    }
    pub fn tested(&self) -> usize {
        self.cases
            .iter()
            .filter(|c| matches!(c.status.as_str(), "Passed" | "Failed"))
            .count()
    }
}

#[derive(Clone)]
pub(super) struct Kubebench {
    data: PathBuf,
    results: PathBuf,
    live: Arc<RwLock<HashMap<Uuid, Report>>>,
}
impl Default for Kubebench {
    fn default() -> Self {
        Self {
            data: env::var_os("ARENA_KUBEBENCH_TASKS")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../kubebench-tasks")
                }),
            results: env::var_os("ARENA_KUBEBENCH_RESULTS")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    dirs::data_local_dir()
                        .unwrap_or_else(env::temp_dir)
                        .join("ai-arena/kubebench")
                }),
            live: Default::default(),
        }
    }
}

impl Kubebench {
    pub async fn catalog(&self) -> Result<Vec<Task>, String> {
        tasks::load(&self.data).await
    }

    pub async fn start(
        &self,
        agents: Vec<(String, AIAgent)>,
        selection: Vec<Task>,
        mode: ArenaMode,
        matrix: bool,
    ) -> io::Result<Uuid> {
        let id = Uuid::new_v4();
        let mut cases = Vec::new();
        for (index, (model, _)) in agents.iter().enumerate() {
            for task in &selection {
                cases.push(Case {
                    task: task.id.clone(),
                    title: task.title.clone(),
                    goal: task.goal.clone(),
                    agent: if matrix {
                        format!("Agent {} · {model}", index + 1)
                    } else {
                        String::new()
                    },
                    status: "Queued".into(),
                    action: String::new(),
                    evidence: String::new(),
                });
            }
        }
        let report = Report {
            id,
            started: Utc::now(),
            matrix,
            model: if matrix {
                format!("{} agents × {} tasks", agents.len(), selection.len())
            } else {
                agents[0].0.clone()
            },
            cluster: "kind".into(),
            mode,
            running: true,
            phase: "Queued".into(),
            error: String::new(),
            cases,
        };
        self.publish(&report).await;
        if let Err(e) = self.checkpoint(&report).await {
            self.live.write().await.remove(&id);
            return Err(e);
        }
        let bench = self.clone();
        tokio::spawn(async move {
            bench.execute(agents, report, selection).await;
        });
        Ok(id)
    }
    async fn publish(&self, report: &Report) {
        self.live.write().await.insert(report.id, report.clone());
    }
    async fn execute(&self, agents: Vec<(String, AIAgent)>, mut report: Report, tasks: Vec<Task>) {
        for (agent_index, (_, agent)) in agents.iter().enumerate() {
            for (task_index, task) in tasks.iter().enumerate() {
                let index = agent_index * tasks.len() + task_index;
                if let Err(error) = self.run_case(agent, &mut report, index, task).await {
                    report.error = error;
                    self.finish(report).await;
                    return;
                }
                if let Err(e) = self.checkpoint(&report).await {
                    report.error = format!("Could not save progress: {e}");
                    self.finish(report).await;
                    return;
                }
            }
        }
        self.finish(report).await;
    }
    async fn checkpoint(&self, report: &Report) -> io::Result<()> {
        if report.mode == ArenaMode::Real && report.matrix {
            self.save(report).await?;
        }
        Ok(())
    }
    async fn finish(&self, mut report: Report) {
        for case in &mut report.cases {
            if case.status == "Queued" {
                case.status = "Skipped".into();
            }
        }
        report.running = false;
        report.phase = if !report.error.is_empty() {
            "Stopped"
        } else if report.cases.iter().any(|c| c.status == "Error") {
            "Finished with errors"
        } else {
            "Finished"
        }
        .into();
        if report.mode == ArenaMode::Real
            && let Err(e) = self.save(&report).await
        {
            report.error = format!("{} Could not save report: {e}", report.error)
                .trim()
                .to_owned();
        }
        self.publish(&report).await;
    }
    async fn run_case(
        &self,
        agent: &AIAgent,
        report: &mut Report,
        i: usize,
        task: &Task,
    ) -> Result<(), String> {
        let name = format!("kb-{}-{i}", report.id.simple());
        let cluster = Cluster::new(name.clone())?;
        report.cases[i].status = "Running".into();
        report.phase = format!("Creating kind cluster {name}");
        if let Err(e) = self.checkpoint(report).await {
            report.cases[i].status = "Skipped".into();
            return Err(format!("Could not save progress: {e}"));
        }
        self.publish(report).await;
        let outcome = async {
            cluster.provision(task.cluster.workers).await?;
            report.phase = format!("Preparing · {}", task.title);
            self.publish(report).await;
            cluster.create("benchmark").await?;
            cluster.seed("benchmark", task).await?;
            self.solve(&cluster, agent, report, i, task).await
        }
        .await;
        match &outcome {
            Ok((passed, evidence)) => {
                report.cases[i].status = if *passed { "Passed" } else { "Failed" }.into();
                report.cases[i].evidence = evidence.clone();
            }
            Err(e) => {
                report.cases[i].status = "Error".into();
                report.cases[i].evidence = e.clone();
            }
        }
        report.phase = format!("Deleting kind cluster {name}");
        self.publish(report).await;
        cluster.teardown().await?;
        if !report.matrix {
            outcome?;
        }
        Ok(())
    }

    async fn solve(
        &self,
        cluster: &Cluster,
        agent: &AIAgent,
        report: &mut Report,
        index: usize,
        task: &Task,
    ) -> Result<(bool, String), String> {
        report.phase = format!("Solving · {}", task.title);
        self.publish(report).await;
        let observed = cluster.observe("benchmark").await?;
        let request = MoveRequest {
            turn_index: 0,
            game_id: "kubebench".into(),
            state: json!({"task":task.goal, "resources":observed}),
            expected_move_schema: action_schema(),
            instructions: Some("You operate Kubernetes workloads. Inspect the supplied resources and return one JSON action to accomplish the task.".into()),
        };
        let answer = timeout(Duration::from_secs(90), agent.execute_turn(&request))
            .await
            .map_err(|_| "Model request timed out.")?;
        let value = match answer {
            Ok(response) => response.chosen_move,
            Err(AgentError::InvalidResponse(_)) => {
                return Ok((false, "The model did not return a JSON action.".into()));
            }
            Err(_) => {
                return Err(
                    "Model request failed. Check the model, endpoint and credentials.".into(),
                );
            }
        };
        report.cases[index].action = value.to_string().chars().take(2048).collect();
        let action = match serde_json::from_value::<Action>(value) {
            Ok(action) => action,
            Err(_) => {
                return Ok((
                    false,
                    "Response does not match an available Kubernetes action.".into(),
                ));
            }
        };
        let (kind, name, patch) = match action.patch(&observed) {
            Ok(patch) => patch,
            Err(reason) => return Ok((false, reason)),
        };
        report.phase = format!("Grading · {}", task.title);
        self.publish(report).await;
        cluster.apply("benchmark", kind, &name, &patch).await?;
        let deadline = Instant::now() + Duration::from_secs(35);
        loop {
            let current = cluster.observe("benchmark").await?;
            let result = task.grade(&current);
            if result.0 || Instant::now() >= deadline {
                return Ok(result);
            }
            sleep(Duration::from_secs(1)).await;
        }
    }

    async fn save(&self, report: &Report) -> io::Result<()> {
        fs::create_dir_all(&self.results).await?;
        let path = self.results.join(format!("{}.json", report.id));
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, serde_json::to_vec(report)?).await?;
        fs::rename(tmp, path).await
    }
    pub async fn get(&self, id: Uuid) -> io::Result<Report> {
        if let Some(report) = self.live.read().await.get(&id) {
            return Ok(report.clone());
        }
        Report::from_disk(&fs::read(self.results.join(format!("{id}.json"))).await?)
    }
    pub async fn history(&self) -> io::Result<Vec<Report>> {
        let mut reports = self.live.read().await.clone();
        match fs::read_dir(&self.results).await {
            Ok(mut dir) => {
                while let Some(entry) = dir.next_entry().await? {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "json")
                        && let Some(id) = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .and_then(|s| Uuid::parse_str(s).ok())
                        && let std::collections::hash_map::Entry::Vacant(slot) = reports.entry(id)
                    {
                        slot.insert(self.get(id).await?);
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let mut reports = reports.into_values().collect::<Vec<_>>();
        reports.sort_by_key(|r| std::cmp::Reverse(r.started));
        Ok(reports)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn only_saved_reports_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let bench = Kubebench {
            results: dir.path().join("results"),
            ..Default::default()
        };
        let test_id = Uuid::new_v4();
        let mut report = Report {
            id: test_id,
            started: Utc::now(),
            model: "fixture".into(),
            matrix: true,
            cluster: "kind-fixture".into(),
            mode: ArenaMode::Test,
            running: true,
            phase: String::new(),
            error: String::new(),
            cases: Vec::new(),
        };
        bench.checkpoint(&report).await.unwrap();
        bench.finish(report.clone()).await;
        assert!(bench.get(test_id).await.is_ok());
        assert_eq!(bench.history().await.unwrap()[0].id, test_id);
        assert!(!bench.results.exists());
        report.id = Uuid::new_v4();
        let saved_id = report.id;
        report.mode = ArenaMode::Real;
        bench.finish(report).await;

        let restarted = Kubebench {
            live: Default::default(),
            ..bench
        };
        assert!(
            matches!(restarted.get(test_id).await, Err(e) if e.kind() == io::ErrorKind::NotFound)
        );
        let history = restarted.history().await.unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].id, saved_id);
        assert!(!restarted.get(saved_id).await.unwrap().running);
    }

    #[test]
    fn history_reads_older_singles_and_interrupted_matrices() {
        let mut stored = json!({
            "id": Uuid::new_v4(), "started": Utc::now(), "mode": "real",
            "cluster":"kind", "model":"fixture", "running":false,
            "phase":"Finished","error":"","cases":[
                {"task":"demo","status":"Passed","action":"{}","evidence":"ok"}
            ]
        });
        let read = |value: &serde_json::Value| {
            Report::from_disk(&serde_json::to_vec(value).unwrap()).unwrap()
        };
        assert_eq!(read(&stored).kind(), "Single");
        stored["running"] = json!(true);
        stored["matrix"] = json!(true);
        for status in ["Running", "Queued"] {
            stored["cases"]
                .as_array_mut()
                .unwrap()
                .push(json!({"task":"demo","status":status,"action":"","evidence":""}));
        }
        let run = read(&stored);
        assert!(!run.running);
        assert_eq!(run.phase, "Interrupted");
        assert_eq!(run.passed(), 1);
        assert_eq!(run.cases[1].status, "Interrupted");
        assert_eq!(run.cases[2].status, "Skipped");
    }
}
