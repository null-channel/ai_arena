//! Adapter for the separately installed toolbench CLI. Private inputs are read by
//! the runner; its output and Arena's run index live together outside the checkout.
pub(super) mod catalog;

use super::state::ArenaMode;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    env, io,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    fs,
    io::{AsyncBufReadExt, BufReader},
    process::Command,
    sync::Mutex,
};
use uuid::Uuid;

#[derive(Clone)]
pub(super) struct Toolbench {
    pub binary: PathBuf,
    pub data: PathBuf,
    pub results: PathBuf,
    progress: Arc<Mutex<HashMap<Uuid, Progress>>>,
    test_runs: Arc<Mutex<HashMap<Uuid, TestRun>>>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Progress {
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub(super) struct Settings {
    pub models: String,
    pub subset: String,
    pub server: String,
    pub profiles: String,
    pub lite: bool,
    pub dry_run: bool,
}

impl Settings {
    fn models(&self) -> impl Iterator<Item = &str> {
        self.models
            .split(|c: char| c == ',' || c.is_whitespace())
            .filter(|s| !s.is_empty())
    }

    pub fn label(&self) -> String {
        if self.dry_run {
            "Offline mock".into()
        } else {
            self.models().collect::<Vec<_>>().join(", ")
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.dry_run {
            return Ok(());
        }
        if self.models().next().is_none() {
            return Err("Enter at least one provider:model or select Offline mock.".into());
        }
        for model in self.models() {
            if !model
                .split_once(':')
                .is_some_and(|(p, m)| !p.is_empty() && !p.starts_with('-') && !m.is_empty())
            {
                return Err(format!("Use provider:model for {model}."));
            }
        }
        if !self.server.trim().is_empty()
            && !self
                .models()
                .all(|m| m.to_ascii_lowercase().starts_with("ollama:"))
        {
            return Err(
                "A custom Ollama endpoint requires all selected models to use Ollama.".into(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Run {
    #[serde(skip)]
    pub test_run: bool,
    pub id: Uuid,
    pub started: u64,
    pub settings: Settings,
    pub status: Status,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) enum Status {
    Running,
    Completed,
    Failed(String),
    Interrupted,
}

impl Status {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::Completed => "Completed",
            Self::Failed(_) => "Failed",
            Self::Interrupted => "Interrupted",
        }
    }
}

#[derive(Clone, Deserialize)]
pub(super) struct Report {
    pub generated_at: String,
    pub models: Vec<ModelScore>,
    pub cases: Vec<CaseScore>,
}

#[derive(Clone, Deserialize)]
pub(super) struct ModelScore {
    pub model: String,
    pub pass: usize,
    pub total: usize,
    pub wrong_tool: usize,
    pub wrong_order: usize,
    pub incomplete: usize,
    pub no_tool_called: usize,
    pub error: usize,
    pub accuracy: f64,
}

#[derive(Clone, Deserialize)]
pub(super) struct CaseScore {
    pub test_id: String,
    pub model: String,
    pub outcome: String,
    pub expected_tools: Vec<String>,
    pub all_picks: Vec<String>,
}

struct TestRun {
    run: Run,
    report: Option<Report>,
}

impl Default for Toolbench {
    fn default() -> Self {
        let siblings = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let binary = env::var_os("ARENA_TOOLBENCH_BIN")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                ["release", "debug"]
                    .into_iter()
                    .map(|profile| {
                        siblings
                            .join("tool_call_tests/target")
                            .join(profile)
                            .join("toolbench")
                    })
                    .find(|p| p.is_file())
                    .unwrap_or_else(|| "toolbench".into())
            });
        Self {
            binary,
            data: env::var_os("ARENA_TOOLBENCH_DATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| siblings.join("tool-call-test-data")),
            results: env::var_os("ARENA_TOOLBENCH_RESULTS")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    dirs::data_local_dir()
                        .unwrap_or_else(env::temp_dir)
                        .join("ai-arena/toolbench")
                }),
            test_runs: Default::default(),
            progress: Default::default(),
        }
    }
}

impl Toolbench {
    pub async fn start(&self, settings: Settings, mode: ArenaMode) -> Result<Uuid, String> {
        settings.validate()?;
        let data = fs::canonicalize(&self.data).await.map_err(|_| {
            "Set ARENA_TOOLBENCH_DATA to the local tool-call-test-data directory.".to_string()
        })?;
        let tools = data.join("tools");
        let cases = data.join("test_cases");
        if !tools.is_dir() || !cases.is_dir() {
            return Err("ARENA_TOOLBENCH_DATA must contain tools/ and test_cases/.".into());
        }
        let id = Uuid::new_v4();
        let mut command = Command::new(&self.binary);
        command
            .arg("run")
            .arg("--no-upload")
            .arg("--tools-dir")
            .arg(tools)
            .arg("--test-cases-dir")
            .arg(cases)
            .env("NO_COLOR", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .kill_on_drop(true);
        let dir = if mode == ArenaMode::Real {
            let dir = self.run_directory(&data, id).await?;
            let stderr = fs::File::create(dir.join("runner.log"))
                .await
                .map_err(|e| e.to_string())?;
            command
                .arg("--reports-dir")
                .arg(&dir)
                .arg("--log-path")
                .arg(dir.join("calls.jsonl"))
                .stderr(stderr.into_std().await);
            Some(dir)
        } else {
            command.arg("--no-save").stderr(Stdio::null());
            None
        };
        if settings.dry_run {
            command.arg("--dry-run");
        } else {
            for model in settings.models() {
                command.arg("--model").arg(model);
            }
            for profile in settings
                .profiles
                .split(',')
                .map(str::trim)
                .filter(|p| !p.is_empty())
            {
                command.arg("--secret").arg(profile);
            }
            if !settings.server.trim().is_empty() {
                command.arg("--server").arg(settings.server.trim());
            }
        }
        if !settings.subset.trim().is_empty() {
            command.arg("--subset").arg(
                settings
                    .subset
                    .split(',')
                    .map(str::trim)
                    .collect::<Vec<_>>()
                    .join(","),
            );
        }
        if settings.lite {
            command.arg("--lite");
        }
        let mut run = Run {
            test_run: mode == ArenaMode::Test,
            id,
            settings,
            started: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            status: Status::Running,
        };
        let mut progress = self.progress.lock().await;
        self.update_run(dir.as_deref(), &run, None)
            .await
            .map_err(|e| e.to_string())?;
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(e) => {
                run.status = Status::Failed(format!(
                    "Could not start toolbench: {e}. Check ARENA_TOOLBENCH_BIN."
                ));
                self.update_run(dir.as_deref(), &run, None)
                    .await
                    .map_err(|e| e.to_string())?;
                return Ok(id);
            }
        };
        progress.insert(id, Progress::default());
        drop(progress);
        let bench = self.clone();
        tokio::spawn(async move {
            let model_count = if run.settings.dry_run {
                1
            } else {
                run.settings.models().count()
            };
            let mut lines = BufReader::new(child.stdout.take().expect("piped stdout")).lines();
            let mut streamed_report = None;
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some((_, total)) = progress_counter(&line)
                    && let Some(p) = bench.progress.lock().await.get_mut(&id)
                {
                    p.done += 1;
                    p.total = total.saturating_mul(model_count);
                } else if dir.is_none()
                    && line.starts_with('{')
                    && let Ok(report) = serde_json::from_str::<Report>(&line)
                {
                    streamed_report = Some(report);
                }
            }
            drop(lines);
            run.status = match child.wait().await {
                Ok(status) if status.success() => {
                    let report = match &dir {
                        Some(path) => read_report(path).await.map(|r| !r.cases.is_empty()),
                        None => streamed_report
                            .as_ref()
                            .map(|r| !r.cases.is_empty())
                            .ok_or_else(|| io::Error::other("Missing streamed report")),
                    };
                    match report {
                        Ok(true) => Status::Completed,
                        Ok(false) => {
                            Status::Failed("The selected dataset contained no scored cases.".into())
                        }
                        Err(e) => Status::Failed(format!("Report unavailable: {e}")),
                    }
                }
                Ok(status) => Status::Failed(match &dir {
                    Some(path) => format!(
                        "Toolbench exited with {status}. See {}.",
                        path.join("runner.log").display()
                    ),
                    None => format!(
                        "Toolbench exited with {status}. Check that the runner supports --no-save and the model settings are valid."
                    ),
                }),
                Err(e) => Status::Failed(format!("Could not wait for toolbench: {e}")),
            };
            if let Err(e) = bench
                .update_run(dir.as_deref(), &run, streamed_report)
                .await
            {
                eprintln!("Could not save toolbench status for {id}: {e}");
            }
            bench.progress.lock().await.remove(&id);
        });
        Ok(id)
    }

    async fn run_directory(&self, data: &Path, id: Uuid) -> Result<PathBuf, String> {
        fs::create_dir_all(&self.results)
            .await
            .map_err(|e| format!("Cannot create the results directory: {e}"))?;
        let root = fs::canonicalize(&self.results)
            .await
            .map_err(|e| e.to_string())?;
        if root.starts_with(data) || root.starts_with(Path::new(env!("CARGO_MANIFEST_DIR"))) {
            return Err("Choose an ARENA_TOOLBENCH_RESULTS directory outside Arena and the private dataset.".into());
        }
        let dir = root.join(id.to_string());
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        builder.mode(0o700);
        builder.create(&dir).await.map_err(|e| e.to_string())?;
        Ok(dir)
    }

    async fn update_run(
        &self,
        dir: Option<&Path>,
        run: &Run,
        report: Option<Report>,
    ) -> io::Result<()> {
        if let Some(dir) = dir {
            save(dir, run).await
        } else {
            self.test_runs.lock().await.insert(
                run.id,
                TestRun {
                    run: run.clone(),
                    report,
                },
            );
            Ok(())
        }
    }

    pub async fn get(&self, id: Uuid) -> io::Result<(Run, Progress)> {
        let live = self.progress.lock().await;
        if let Some(test) = self.test_runs.lock().await.get(&id) {
            return Ok((test.run.clone(), live.get(&id).copied().unwrap_or_default()));
        }
        let mut run: Run = serde_json::from_slice(
            &fs::read(self.results.join(id.to_string()).join("arena.json")).await?,
        )?;
        if matches!(run.status, Status::Running) && !live.contains_key(&id) {
            run.status = Status::Interrupted;
        }
        Ok((run, live.get(&id).copied().unwrap_or_default()))
    }

    pub async fn history(&self) -> io::Result<Vec<Run>> {
        let mut dirs = match fs::read_dir(&self.results).await {
            Ok(dirs) => dirs,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let mut runs = Vec::new();
        while let Some(entry) = dirs.next_entry().await? {
            if let Ok(id) = Uuid::parse_str(&entry.file_name().to_string_lossy()) {
                match self.get(id).await {
                    Ok((run, _)) => runs.push(run),
                    Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                    Err(e) => return Err(e),
                }
            }
        }
        runs.sort_by_key(|r| std::cmp::Reverse((r.started, r.id)));
        Ok(runs)
    }

    pub async fn report(&self, id: Uuid) -> io::Result<Report> {
        if let Some(test) = self.test_runs.lock().await.get(&id) {
            return test
                .report
                .clone()
                .ok_or_else(|| io::Error::other("Report unavailable"));
        }
        read_report(&self.results.join(id.to_string())).await
    }
}

fn progress_counter(line: &str) -> Option<(usize, usize)> {
    let (prefix, _) = line.strip_prefix('[')?.split_once(']')?;
    let (n, total) = prefix.split_once('/')?;
    Some((n.trim().parse().ok()?, total.parse().ok()?))
}

async fn read_report(dir: &Path) -> io::Result<Report> {
    let mut files = fs::read_dir(dir).await?;
    while let Some(file) = files.next_entry().await? {
        let name = file.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("run-") && name.ends_with(".json") {
            return Ok(serde_json::from_slice(&fs::read(file.path()).await?)?);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::NotFound,
        "toolbench did not produce a run report",
    ))
}

async fn save(dir: &Path, run: &Run) -> io::Result<()> {
    let tmp = dir.join("arena.tmp");
    fs::write(&tmp, serde_json::to_vec(run)?).await?;
    fs::rename(tmp, dir.join("arena.json")).await
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn runs_in_isolated_directory_and_reopens_result_after_restart() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        let bench = Toolbench {
            binary: root.join("test-runner"),
            data: root.join("inputs"),
            results: root.join("outputs"),
            test_runs: Default::default(),
            progress: Default::default(),
        };
        fs::create_dir_all(bench.data.join("tools")).await.unwrap();
        fs::create_dir_all(bench.data.join("test_cases"))
            .await
            .unwrap();
        // Stand-in for the external CLI. CI needs neither the sibling checkout nor private cases.
        fs::write(&bench.binary, r#"#!/bin/sh
set -eu
test "$1" = run && test "$2" = --no-upload
shift 2
while [ "$#" -gt 0 ]; do
    case "$1" in
        --tools-dir|--test-cases-dir) test -d "$2"; shift 2 ;;
        --log-path) log=$2; shift 2 ;;
        --reports-dir) reports=$2; shift 2 ;;
        --no-save) ephemeral=yes; shift ;;
        --dry-run) mock=yes; shift ;;
        *) exit 2 ;;
    esac
done
test "$mock" = yes
echo '[   1/1] PASS'
report='{"generated_at":"today","models":[],"cases":[{"test_id":"fixture","model":"mock","outcome":"pass","expected_tools":[],"all_picks":[]}]}'
if [ "${ephemeral:-}" = yes ]; then
    test -z "${reports:-}${log:-}"
    echo "$report"
else
    test "$log" = "$reports/calls.jsonl"
    echo "$report" > "$reports/run-test.json"
fi
"#).await.unwrap();
        fs::set_permissions(&bench.binary, std::fs::Permissions::from_mode(0o700))
            .await
            .unwrap();
        let mut ids = Vec::new();
        for mode in [ArenaMode::Test, ArenaMode::Real] {
            let id = bench
                .start(
                    Settings {
                        dry_run: true,
                        ..Default::default()
                    },
                    mode,
                )
                .await
                .unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(5), async {
                loop {
                    let (run, _) = bench.get(id).await.unwrap();
                    match run.status {
                        Status::Running => {
                            tokio::time::sleep(std::time::Duration::from_millis(10)).await
                        }
                        Status::Completed => break,
                        other => panic!("unexpected run status: {other:?}"),
                    }
                }
            })
            .await
            .unwrap();
            ids.push(id);
            assert_eq!(bench.report(id).await.unwrap().cases[0].outcome, "pass");
            if mode == ArenaMode::Test {
                assert!(!bench.results.exists());
                assert!(bench.history().await.unwrap().is_empty());
            }
        }
        let reopened = Toolbench {
            test_runs: Default::default(),
            progress: Default::default(),
            ..bench
        };
        assert!(reopened.get(ids[0]).await.is_err());
        assert_eq!(reopened.history().await.unwrap()[0].id, ids[1]);
        assert_eq!(
            reopened.report(ids[1]).await.unwrap().cases[0].outcome,
            "pass"
        );
    }
}
