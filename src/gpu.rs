use crate::error::AppError;

/// Inject GPU device access into a Docker Compose YAML string.
///
/// - **nvidia**: Adds `deploy.resources.reservations.devices` with driver nvidia.
/// - **intel**: Adds `devices: ["/dev/dri:/dev/dri"]` for Intel/AMD iGPU.
pub fn inject_gpu(
    compose_yaml: &str,
    compose_keys: &[String],
    mode: &str,
) -> Result<String, AppError> {
    let mut doc: serde_yaml::Value = serde_yaml::from_str(compose_yaml)
        .map_err(|e| AppError::Compose(format!("Failed to parse compose YAML: {e}")))?;

    let services = doc
        .get_mut("services")
        .and_then(|s| s.as_mapping_mut())
        .ok_or_else(|| AppError::Compose("No 'services' key in compose YAML".into()))?;

    for key in compose_keys {
        let Some(svc) = services.get_mut(serde_yaml::Value::String(key.clone())) else {
            continue;
        };
        let svc_map = svc
            .as_mapping_mut()
            .ok_or_else(|| AppError::Compose(format!("Compose key '{key}' is not a mapping")))?;

        match mode {
            "nvidia" => inject_nvidia(svc_map),
            "intel" => inject_intel(svc_map),
            _ => return Err(AppError::Compose(format!("Unknown GPU mode: {mode}"))),
        }
    }

    serde_yaml::to_string(&doc)
        .map_err(|e| AppError::Compose(format!("Failed to serialize compose YAML: {e}")))
}

fn inject_nvidia(svc: &mut serde_yaml::Mapping) {
    // deploy.resources.reservations.devices
    let device: serde_yaml::Value = serde_yaml::from_str(
        r#"
- driver: nvidia
  count: all
  capabilities:
    - gpu
"#,
    )
    .expect("static YAML");

    let deploy = svc
        .entry(serde_yaml::Value::String("deploy".into()))
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    let resources = deploy
        .as_mapping_mut()
        .unwrap()
        .entry(serde_yaml::Value::String("resources".into()))
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    let reservations = resources
        .as_mapping_mut()
        .unwrap()
        .entry(serde_yaml::Value::String("reservations".into()))
        .or_insert_with(|| serde_yaml::Value::Mapping(serde_yaml::Mapping::new()));
    reservations
        .as_mapping_mut()
        .unwrap()
        .insert(serde_yaml::Value::String("devices".into()), device);
}

fn inject_intel(svc: &mut serde_yaml::Mapping) {
    let existing = svc
        .entry(serde_yaml::Value::String("devices".into()))
        .or_insert_with(|| serde_yaml::Value::Sequence(Vec::new()));

    let seq = existing.as_sequence_mut().unwrap();
    let dri = serde_yaml::Value::String("/dev/dri:/dev/dri".into());
    if !seq.contains(&dri) {
        seq.push(dri);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASIC_COMPOSE: &str = r#"
services:
  app:
    image: myapp:latest
    ports:
      - "8080:8080"
  worker:
    image: myworker:latest
"#;

    #[test]
    fn inject_nvidia_adds_deploy_block() {
        let result = inject_gpu(
            BASIC_COMPOSE,
            &["app".to_string()],
            "nvidia",
        )
        .unwrap();

        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();
        let app = &doc["services"]["app"];
        let devices = &app["deploy"]["resources"]["reservations"]["devices"];
        assert!(devices.is_sequence());
        let first = &devices[0];
        assert_eq!(first["driver"].as_str().unwrap(), "nvidia");
        assert_eq!(first["count"].as_str().unwrap(), "all");
        let caps = first["capabilities"].as_sequence().unwrap();
        assert_eq!(caps[0].as_str().unwrap(), "gpu");
    }

    #[test]
    fn inject_intel_adds_devices() {
        let result = inject_gpu(
            BASIC_COMPOSE,
            &["app".to_string()],
            "intel",
        )
        .unwrap();

        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();
        let devices = doc["services"]["app"]["devices"].as_sequence().unwrap();
        assert!(devices.iter().any(|d| d.as_str() == Some("/dev/dri:/dev/dri")));
    }

    #[test]
    fn inject_gpu_multiple_apps() {
        let result = inject_gpu(
            BASIC_COMPOSE,
            &["app".to_string(), "worker".to_string()],
            "nvidia",
        )
        .unwrap();

        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();
        assert!(doc["services"]["app"]["deploy"]["resources"]["reservations"]["devices"].is_sequence());
        assert!(doc["services"]["worker"]["deploy"]["resources"]["reservations"]["devices"].is_sequence());
    }

    #[test]
    fn inject_gpu_skips_missing_key() {
        let result = inject_gpu(
            BASIC_COMPOSE,
            &["nonexistent".to_string()],
            "nvidia",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn inject_gpu_unknown_mode_errors() {
        let result = inject_gpu(
            BASIC_COMPOSE,
            &["app".to_string()],
            "amd",
        );
        assert!(result.is_err());
    }

    #[test]
    fn inject_intel_no_duplicate() {
        // Inject twice, should not duplicate
        let first = inject_gpu(BASIC_COMPOSE, &["app".to_string()], "intel").unwrap();
        let second = inject_gpu(&first, &["app".to_string()], "intel").unwrap();

        let doc: serde_yaml::Value = serde_yaml::from_str(&second).unwrap();
        let devices = doc["services"]["app"]["devices"].as_sequence().unwrap();
        let dri_count = devices
            .iter()
            .filter(|d| d.as_str() == Some("/dev/dri:/dev/dri"))
            .count();
        assert_eq!(dri_count, 1);
    }

    #[test]
    fn inject_nvidia_preserves_existing_ports() {
        let result = inject_gpu(
            BASIC_COMPOSE,
            &["app".to_string()],
            "nvidia",
        )
        .unwrap();

        let doc: serde_yaml::Value = serde_yaml::from_str(&result).unwrap();
        let ports = doc["services"]["app"]["ports"].as_sequence().unwrap();
        assert!(!ports.is_empty());
    }
}
