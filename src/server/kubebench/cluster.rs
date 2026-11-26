use super::tasks::{Task, snapshot};
use serde_json::{Value, json};
use std::{process::Stdio, time::Duration};
use tempfile::TempDir;
use tokio::{fs, io::AsyncWriteExt, process::Command, time::timeout};

pub(super) struct Cluster {
    name: String,
    workspace: TempDir,
    config: tempfile::NamedTempFile,
}

async fn command(
    cmd: &mut Command,
    input: Option<&Value>,
    seconds: u64,
) -> Result<Vec<u8>, String> {
    cmd.kill_on_drop(true)
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| {
        format!(
            "Could not start {}: {e}",
            cmd.as_std().get_program().to_string_lossy()
        )
    })?;
    let output = timeout(Duration::from_secs(seconds), async {
        if let Some(input) = input {
            child
                .stdin
                .take()
                .expect("piped stdin")
                .write_all(&serde_json::to_vec(input)?)
                .await?;
        }
        child.wait_with_output().await
    })
    .await
    .map_err(|_| "Cluster command timed out.".to_owned())?
    .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .trim()
            .chars()
            .take(800)
            .collect());
    }
    Ok(output.stdout)
}

impl Cluster {
    pub fn new(name: String) -> Result<Self, String> {
        let workspace = tempfile::tempdir().map_err(|e| e.to_string())?;
        let config =
            tempfile::NamedTempFile::new_in(workspace.path()).map_err(|e| e.to_string())?;
        Ok(Self {
            name,
            workspace,
            config,
        })
    }

    fn kind(&self, operation: &str) -> Command {
        let mut kind = Command::new("kind");
        kind.args([operation, "cluster", "--name", &self.name, "--kubeconfig"])
            .arg(self.config.path());
        kind
    }

    pub async fn provision(&self, workers: u8) -> Result<(), String> {
        let mut nodes = vec![json!({"role": "control-plane"})];
        nodes.extend((0..workers).map(|_| json!({"role": "worker"})));
        let config = json!({"kind":"Cluster","apiVersion":"kind.x-k8s.io/v1alpha4","nodes":nodes});
        let path = self.workspace.path().join("kind.json");
        fs::write(&path, config.to_string())
            .await
            .map_err(|e| e.to_string())?;
        let mut create = self.kind("create");
        create.args(["--wait", "120s", "--config"]).arg(path);
        command(&mut create, None, 360)
            .await
            .map(|_| ())
            .map_err(|e| format!("Could not create kind cluster {}: {e}", self.name))
    }

    pub async fn teardown(&self) -> Result<(), String> {
        command(&mut self.kind("delete"), None, 120)
            .await
            .map(|_| ())
            .map_err(|e| format!("Could not delete kind cluster {}: {e}", self.name))
    }

    async fn pause_image(&self) -> Result<String, String> {
        // The kind node already has its Pod sandbox image, so the suite works offline.
        let nodes = self.run(&["get", "nodes", "-o", "json"], None, 25).await?;
        let list: Value = serde_json::from_slice(&nodes).map_err(|e| e.to_string())?;
        Ok(list["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|n| n["status"]["images"].as_array())
            .flatten()
            .filter_map(|i| i["names"].as_array())
            .flatten()
            .filter_map(Value::as_str)
            .find(|n| n.starts_with("registry.k8s.io/pause:"))
            .unwrap_or("registry.k8s.io/pause:3.9")
            .to_string())
    }

    async fn run(
        &self,
        args: &[&str],
        input: Option<&Value>,
        seconds: u64,
    ) -> Result<Vec<u8>, String> {
        command(
            Command::new("kubectl")
                .arg("--kubeconfig")
                .arg(self.config.path())
                .arg("--cache-dir")
                .arg(self.workspace.path().join("cache"))
                .arg("--request-timeout=20s")
                .args(args),
            input,
            seconds,
        )
        .await
    }
    pub async fn create(&self, ns: &str) -> Result<(), String> {
        self.run(&["create", "namespace", ns], None, 25)
            .await
            .map(|_| ())
    }
    pub async fn seed(&self, ns: &str, task: &Task) -> Result<(), String> {
        let mut resources = task.resources.clone();
        let mut pause_image = None;
        for resource in &mut resources {
            let spec = if resource["kind"] == "Deployment" {
                resource.pointer_mut("/spec/template/spec")
            } else if resource["kind"] == "Pod" {
                resource.get_mut("spec")
            } else {
                None
            };
            if let Some(containers) = spec
                .and_then(|spec| spec.get_mut("containers"))
                .and_then(Value::as_array_mut)
            {
                for container in containers {
                    if container["image"] == "$KUBEBENCH_PAUSE_IMAGE" {
                        if pause_image.is_none() {
                            pause_image = Some(self.pause_image().await?);
                        }
                        container["image"] = json!(pause_image);
                    }
                }
            }
        }
        self.run(
            &["-n", ns, "create", "-f", "-"],
            Some(&json!({"apiVersion":"v1","kind":"List","items":resources})),
            30,
        )
        .await?;
        if resources.iter().any(|r| r["kind"] == "Deployment") {
            self.run(
                &[
                    "-n",
                    ns,
                    "wait",
                    "--for=condition=Available",
                    "deployments",
                    "--all",
                    "--timeout=90s",
                ],
                None,
                100,
            )
            .await?;
        }
        Ok(())
    }
    pub async fn observe(&self, ns: &str) -> Result<Vec<Value>, String> {
        let out = self
            .run(
                &[
                    "-n",
                    ns,
                    "get",
                    "deployments,services,pods,endpointslices,resourcequotas",
                    "-o",
                    "json",
                ],
                None,
                30,
            )
            .await?;
        snapshot(&serde_json::from_slice(&out).map_err(|e| e.to_string())?)
    }
    pub async fn apply(
        &self,
        ns: &str,
        kind: &str,
        name: &str,
        patch: &Value,
    ) -> Result<(), String> {
        self.run(
            &[
                "-n",
                ns,
                "patch",
                kind,
                name,
                "--type=merge",
                "-p",
                &patch.to_string(),
            ],
            None,
            30,
        )
        .await
        .map(|_| ())
    }
}
