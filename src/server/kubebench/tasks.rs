use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use tokio::fs;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::server) struct Task {
    #[serde(skip)]
    pub id: String,
    pub title: String,
    pub goal: String,
    #[serde(default)]
    pub cluster: ClusterSetup,
    pub resources: Vec<Value>,
    checks: Vec<Check>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::server) struct ClusterSetup {
    #[serde(default)]
    pub workers: u8,
}

#[derive(Deserialize)]
#[serde(tag = "check", rename_all = "snake_case", deny_unknown_fields)]
enum Check {
    Field {
        kind: String,
        name: String,
        path: String,
        equals: Value,
    },
    ReadyEndpoints {
        service: String,
        pod_labels: BTreeMap<String, String>,
    },
}

pub(in crate::server) async fn load(dir: &Path) -> Result<Vec<Task>, String> {
    let mut entries = fs::read_dir(dir).await.map_err(|e|
        format!("Cannot read KubeBench tasks at {}. Set ARENA_KUBEBENCH_TASKS to your task directory. {e}", dir.display()))?;
    let mut tasks = Vec::new();
    while let Some(entry) = entries.next_entry().await.map_err(|e| e.to_string())? {
        let path = entry.path();
        if path.extension().is_none_or(|ext| ext != "json")
            || !entry
                .file_type()
                .await
                .map_err(|e| e.to_string())?
                .is_file()
        {
            continue;
        }
        let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        if id.is_empty()
            || id == "all"
            || !id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        {
            return Err(format!(
                "Invalid task ID in {}. Use letters, digits, hyphens or underscores. The ID 'all' is reserved.",
                path.display()
            ));
        }
        let bytes = fs::read(&path)
            .await
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let mut task: Task =
            serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        task.validate()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        task.id = id.to_owned();
        tasks.push(task);
    }
    if tasks.is_empty() {
        return Err(format!("No JSON task files found in {}.", dir.display()));
    }
    tasks.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(tasks)
}

impl Task {
    fn validate(&self) -> Result<(), &'static str> {
        if self.title.trim().is_empty()
            || self.goal.trim().is_empty()
            || self.resources.is_empty()
            || self.checks.is_empty()
        {
            return Err("Provide a title, goal, resources and grading checks.");
        }
        if self.cluster.workers > 3 {
            return Err("A task can request up to three kind workers.");
        }
        for resource in &self.resources {
            if !matches!(
                resource["kind"].as_str(),
                Some("Deployment" | "Service" | "ResourceQuota" | "Pod")
            ) || resource["metadata"]["name"]
                .as_str()
                .is_none_or(str::is_empty)
                || resource["metadata"].get("namespace").is_some()
            {
                return Err(
                    "Fixtures must be named Deployments, Services, ResourceQuotas or Pods in the task namespace. Omit metadata.namespace.",
                );
            }
        }
        for check in &self.checks {
            match check {
                Check::Field {
                    kind, name, path, ..
                } if !matches!(
                    kind.as_str(),
                    "Deployment" | "Service" | "ResourceQuota" | "Pod" | "EndpointSlice"
                ) || name.is_empty()
                    || !path.starts_with('/') =>
                {
                    return Err(
                        "A field check requires a supported resource kind, a name and a JSON pointer.",
                    );
                }
                Check::ReadyEndpoints {
                    service,
                    pod_labels,
                } if service.is_empty() || pod_labels.is_empty() => {
                    return Err("An endpoint check requires a Service name and pod labels.");
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub(super) fn grade(&self, rows: &[Value]) -> (bool, String) {
        let mut passed = true;
        let mut evidence = Vec::new();
        for check in &self.checks {
            let (ok, detail) = check.evaluate(rows);
            passed &= ok;
            evidence.push(detail);
        }
        (passed, evidence.join("\n"))
    }
}

impl Check {
    fn evaluate(&self, rows: &[Value]) -> (bool, String) {
        match self {
            Self::Field {
                kind,
                name,
                path,
                equals,
            } => {
                let actual = rows
                    .iter()
                    .find(|r| r["kind"] == *kind && r["name"] == *name)
                    .and_then(|r| r.pointer(path));
                let value = actual.map_or_else(|| "missing".into(), Value::to_string);
                (
                    actual == Some(equals),
                    format!("{kind}/{name} {path} = {value} (expected {equals})"),
                )
            }
            Self::ReadyEndpoints {
                service,
                pod_labels,
            } => {
                let pods: BTreeSet<_> = rows
                    .iter()
                    .filter(|r| {
                        r["kind"] == "Pod"
                            && pod_labels
                                .iter()
                                .all(|(key, value)| r["labels"][key] == *value)
                    })
                    .filter_map(|r| r["uid"].as_str())
                    .collect();
                let endpoints = rows
                    .iter()
                    .filter(|r| r["kind"] == "EndpointSlice" && r["service"] == *service)
                    .filter_map(|r| r["endpoints"].as_array())
                    .flatten();
                let mut ready = BTreeSet::new();
                let mut selected_only = true;
                for endpoint in endpoints {
                    let uid = endpoint["targetRef"]["uid"].as_str();
                    selected_only &= uid.is_some_and(|uid| pods.contains(uid));
                    if let Some(uid) = uid
                        && endpoint["conditions"]["ready"] == true
                    {
                        ready.insert(uid);
                    }
                }
                (
                    !pods.is_empty() && selected_only && ready == pods,
                    format!(
                        "Service/{service}: {} of {} selected pods have ready endpoints; {} other pods included.",
                        ready.intersection(&pods).count(),
                        pods.len(),
                        if selected_only { "no" } else { "some" }
                    ),
                )
            }
        }
    }
}

// Only these fields from benchmark-owned objects enter the model prompt.
pub(super) fn snapshot(list: &Value) -> Result<Vec<Value>, String> {
    let objects = list["items"]
        .as_array()
        .ok_or("Expected a Kubernetes resource list.")?;
    let mut rows = Vec::new();
    for obj in objects {
        let meta = &obj["metadata"];
        let spec = &obj["spec"];
        let mut row = match obj["kind"].as_str() {
            Some("Deployment") => {
                json!({"desired":spec["replicas"],"available":obj["status"]["availableReplicas"],"podLabels":spec["template"]["metadata"]["labels"]})
            }
            Some("Service") => json!({"selector":spec["selector"]}),
            Some("Pod") => json!({"uid":meta["uid"],"labels":meta["labels"]}),
            Some("EndpointSlice") => {
                json!({"service":meta["labels"]["kubernetes.io/service-name"],"endpoints":obj["endpoints"]})
            }
            Some("ResourceQuota") => json!({"hard":spec["hard"]}),
            _ => continue,
        };
        row["kind"] = obj["kind"].clone();
        row["name"] = meta["name"].clone();
        rows.push(row);
    }
    Ok(rows)
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Action {
    Scale {
        name: String,
        replicas: u8,
    },
    SetSelector {
        name: String,
        key: String,
        value: String,
    },
    SetLimit {
        name: String,
        resource: String,
        limit: u8,
    },
}

impl Action {
    pub(super) fn patch(self, snapshot: &[Value]) -> Result<(&'static str, String, Value), String> {
        let (kind, name, patch) = match self {
            Self::Scale { name, replicas } if replicas <= 5 => {
                ("Deployment", name, json!({"spec":{"replicas":replicas}}))
            }
            Self::SetSelector { name, key, value }
                if matches!(key.as_str(), "app" | "track")
                    && value.len() <= 63
                    && value
                        .as_bytes()
                        .first()
                        .is_some_and(u8::is_ascii_alphanumeric)
                    && value
                        .as_bytes()
                        .last()
                        .is_some_and(u8::is_ascii_alphanumeric)
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_')) =>
            {
                ("Service", name, json!({"spec":{"selector":{(key):value}}}))
            }
            Self::SetLimit {
                name,
                resource,
                limit,
            } if matches!(resource.as_str(), "pods" | "services" | "configmaps") && limit <= 10 => {
                (
                    "ResourceQuota",
                    name,
                    json!({"spec":{"hard":{(resource):limit.to_string()}}}),
                )
            }
            _ => return Err("Action is outside the permitted fields or numeric limits.".into()),
        };
        if !snapshot
            .iter()
            .any(|r| r["kind"] == kind && r["name"] == name)
        {
            return Err(format!("No {kind} named {name} in the task namespace."));
        }
        Ok((kind, name, patch))
    }
}

pub(super) fn action_schema() -> Value {
    json!({"oneOf":[
        {"type":"object","required":["op","name","replicas"],"properties":{"op":{"const":"scale"},"name":{"type":"string"},"replicas":{"type":"integer","minimum":0,"maximum":5}}},
        {"type":"object","required":["op","name","key","value"],"properties":{"op":{"const":"set_selector"},"name":{"type":"string"},"key":{"enum":["app","track"]},"value":{"type":"string","maxLength":63,"pattern":"^[A-Za-z0-9]([-A-Za-z0-9._]*[A-Za-z0-9])?$"}}},
        {"type":"object","required":["op","name","resource","limit"],"properties":{"op":{"const":"set_limit"},"name":{"type":"string"},"resource":{"enum":["configmaps","pods","services"]},"limit":{"type":"integer","minimum":0,"maximum":10}}}
    ]})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn catalog_loads_external_checks_and_snapshots_a_launch() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("custom.json");
        let mut definition: Value = serde_json::from_str(include_str!(
            "../../../examples/kubebench/example-quota.json"
        ))
        .unwrap();
        fs::write(&path, definition.to_string()).await.unwrap();
        let original = load(dir.path()).await.unwrap().pop().unwrap();
        assert_eq!(original.id, "custom");
        let observed = [
            json!({"kind":"ResourceQuota","name":"example-capacity","hard":{"services":"4","pods":"6"}}),
        ];
        assert!(original.grade(&observed).0);

        definition["checks"][0]["equals"]["services"] = json!("5");
        fs::write(&path, definition.to_string()).await.unwrap();
        let edited = load(dir.path()).await.unwrap().pop().unwrap();
        assert!(!edited.grade(&observed).0);
        assert!(original.grade(&observed).0);

        definition["checks"] = json!([]);
        fs::write(&path, definition.to_string()).await.unwrap();
        assert!(
            load(dir.path())
                .await
                .err()
                .unwrap()
                .contains("custom.json")
        );
    }

    #[test]
    fn endpoint_check_matches_identities_and_readiness() {
        let check = Check::ReadyEndpoints {
            service: "demo".into(),
            pod_labels: BTreeMap::from([("component".into(), "demo".into())]),
        };
        let mut rows = [
            json!({"kind":"Pod","uid":"one","labels":{"component":"demo"}}),
            json!({"kind":"EndpointSlice","service":"demo","endpoints":[{"conditions":{"ready":true},"targetRef":{"uid":"different"}}]}),
        ];
        assert!(!check.evaluate(&rows).0);
        rows[1]["endpoints"][0]["targetRef"]["uid"] = json!("one");
        assert!(check.evaluate(&rows).0);
        rows[1]["endpoints"][0]["conditions"]["ready"] = json!(false);
        assert!(!check.evaluate(&rows).0);
    }

    #[test]
    fn actions_only_patch_observed_objects_and_supported_fields() {
        let observed = [
            json!({"kind":"ResourceQuota","name":"capacity"}),
            json!({"kind":"Service","name":"demo"}),
        ];
        let action: Action = serde_json::from_value(
            json!({"op":"set_limit","name":"capacity","resource":"configmaps","limit":5}),
        )
        .unwrap();
        let (_, _, patch) = action.patch(&observed).unwrap();
        assert_eq!(patch, json!({"spec":{"hard":{"configmaps":"5"}}}));
        for v in [
            json!({"op":"scale","name":"capacity","replicas":1}),
            json!({"op":"set_selector","name":"demo","key":"track","value":"-edge-"}),
            json!({"op":"set_limit","name":"elsewhere","resource":"pods","limit":1}),
            json!({"op":"set_limit","name":"capacity","resource":"pods","limit":200}),
            json!({"op":"set_limit","name":"capacity","resource":"pods","limit":1,"namespace":"elsewhere"}),
        ] {
            assert!(
                serde_json::from_value::<Action>(v)
                    .ok()
                    .and_then(|a| a.patch(&observed).ok())
                    .is_none()
            );
        }
    }
}
