#[cfg(test)]
mod tests {
    use crate::actions::Manifest;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_parse_json_manifest() {
        // Create a temporary directory for the test
        let temp_dir = tempdir().unwrap();
        let manifest_path = temp_dir.path().join("test_manifest.json");

        // Create a simple manifest
        let mut manifest = Manifest::new();

        // Add some attributes
        let mut attr = crate::actions::Attr::default();
        attr.key = "pkg.fmri".to_string();
        attr.values = vec!["pkg://test/example@1.0.0".to_string()];
        manifest.attributes.push(attr);

        // Serialize the manifest to JSON
        let manifest_json = serde_json::to_string_pretty(&manifest).unwrap();

        // Write the JSON to a file
        let mut file = File::create(&manifest_path).unwrap();
        file.write_all(manifest_json.as_bytes()).unwrap();

        // Parse the JSON manifest
        let parsed_manifest = Manifest::parse_file(&manifest_path).unwrap();

        // Verify that the parsed manifest matches the original
        assert_eq!(parsed_manifest.attributes.len(), 1);
        assert_eq!(parsed_manifest.attributes[0].key, "pkg.fmri");
        assert_eq!(
            parsed_manifest.attributes[0].values[0],
            "pkg://test/example@1.0.0"
        );
    }

    #[test]
    fn test_parse_string_manifest() {
        // Create a temporary directory for the test
        let temp_dir = tempdir().unwrap();
        let manifest_path = temp_dir.path().join("test_manifest.p5m");

        // Create a simple string-formatted manifest
        let manifest_string = "set name=pkg.fmri value=pkg://test/example@1.0.0\n";

        // Write the string to a file
        let mut file = File::create(&manifest_path).unwrap();
        file.write_all(manifest_string.as_bytes()).unwrap();

        // Parse the string manifest
        let parsed_manifest = Manifest::parse_file(&manifest_path).unwrap();

        // Verify that the parsed manifest has the expected attributes
        assert_eq!(parsed_manifest.attributes.len(), 1);
        assert_eq!(parsed_manifest.attributes[0].key, "pkg.fmri");
        assert_eq!(
            parsed_manifest.attributes[0].values[0],
            "pkg://test/example@1.0.0"
        );
    }
}
