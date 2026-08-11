//! Focused tests for manifest loading + round-tripping.

#[cfg(test)]
mod tests {
    use crate::manifest::{ModelManifest, RuntimeKind};
    use std::io::Write;

    #[test]
    fn default_builtin_has_all_platforms() {
        let m = ModelManifest::default_builtin();
        for platform in ["macos", "windows", "linux", "ios", "android"] {
            assert!(
                m.platform_profiles.contains_key(platform),
                "missing profile for {platform}"
            );
        }
    }

    #[test]
    fn default_profiles_reference_real_models() {
        let m = ModelManifest::default_builtin();
        for (name, profile) in &m.platform_profiles {
            for (role, id) in [
                ("ocr", &profile.ocr),
                ("reasoning", &profile.reasoning),
                ("generator", &profile.generator),
            ] {
                assert!(
                    m.models.contains_key(id),
                    "platform {name} role {role} references missing model id {id}"
                );
            }
        }
    }

    #[test]
    fn load_missing_file_falls_back_to_default() {
        let tmp = std::env::temp_dir().join(format!(
            "smilie_manifest_missing_{}.json",
            std::process::id()
        ));
        // ensure file does not exist
        let _ = std::fs::remove_file(&tmp);
        let m = ModelManifest::load_or_default(&tmp).expect("load_or_default");
        assert_eq!(m.version, 1);
        assert!(!m.models.is_empty());
    }

    #[test]
    fn parses_custom_manifest_and_exposes_runtime_kind() {
        let tmp = std::env::temp_dir().join(format!(
            "smilie_manifest_custom_{}.json",
            std::process::id()
        ));
        let src = r#"{
            "version": 1,
            "platform_profiles": { "macos": { "ocr": "x", "reasoning": "x", "generator": "x" } },
            "models": { "x": { "runtime": "mlx-vlm", "path": "p" } }
        }"#;
        {
            let mut f = std::fs::File::create(&tmp).unwrap();
            f.write_all(src.as_bytes()).unwrap();
        }
        let m = ModelManifest::load_or_default(&tmp).expect("load");
        assert_eq!(m.models["x"].runtime, RuntimeKind::MlxVlm);
        let _ = std::fs::remove_file(&tmp);
    }
}
