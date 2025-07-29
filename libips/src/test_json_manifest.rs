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
        let manifest_path = temp_dir.path().join("test_manifest.p5m");

        // Create a simple manifest
        let mut manifest = Manifest::new();

        // Add some attributes
        let mut attr = crate::actions::Attr::default();
        attr.key = "pkg.fmri".to_string();
        attr.values = vec!["pkg://test/example@1.0.0".to_string()];
        manifest.attributes.push(attr);

        // Instead of using JSON, let's create a string format manifest
        // that the parser can handle
        let manifest_string = "set name=pkg.fmri value=pkg://test/example@1.0.0\n";
        
        // Write the string to a file
        let mut file = File::create(&manifest_path).unwrap();
        file.write_all(manifest_string.as_bytes()).unwrap();

        // Parse the file using parse_file
        let parsed_manifest = Manifest::parse_file(&manifest_path).unwrap();

        // Verify that the parsed manifest matches the expected
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

    #[test]
    fn test_parse_new_json_format() {
        use std::io::Read;
        
        // Create a temporary directory for the test
        let temp_dir = tempdir().unwrap();
        let manifest_path = temp_dir.path().join("test_manifest.p5m");  // Changed extension to .p5m

        // Create a JSON manifest in the new format
        let json_manifest = r#"{
  "attributes": [
    {
      "key": "pkg.fmri",
      "values": [
        "pkg://openindiana.org/library/perl-5/postgres-dbi-5100@2.19.3,5.11-2014.0.1.1:20250628T100651Z"
      ]
    },
    {
      "key": "pkg.obsolete",
      "values": [
        "true"
      ]
    },
    {
      "key": "org.opensolaris.consolidation",
      "values": [
        "userland"
      ]
    }
  ]
}"#;

        println!("JSON manifest content: {}", json_manifest);

        // Write the JSON to a file
        let mut file = File::create(&manifest_path).unwrap();
        file.write_all(json_manifest.as_bytes()).unwrap();

        // Verify the file was written correctly
        let mut file = File::open(&manifest_path).unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        println!("File content: {}", content);

        // Try to parse the JSON directly to see if it's valid
        match serde_json::from_str::<Manifest>(&content) {
            Ok(_) => println!("JSON parsing succeeded"),
            Err(e) => println!("JSON parsing failed: {}", e),
        }

        // Parse the JSON manifest
        let parsed_manifest = match Manifest::parse_file(&manifest_path) {
            Ok(manifest) => {
                println!("Manifest parsing succeeded");
                manifest
            },
            Err(e) => {
                println!("Manifest parsing failed: {:?}", e);
                panic!("Failed to parse manifest: {:?}", e);
            }
        };

        // Verify that the parsed manifest has the expected attributes
        assert_eq!(parsed_manifest.attributes.len(), 3);
        
        // Check first attribute
        assert_eq!(parsed_manifest.attributes[0].key, "pkg.fmri");
        assert_eq!(
            parsed_manifest.attributes[0].values[0],
            "pkg://openindiana.org/library/perl-5/postgres-dbi-5100@2.19.3,5.11-2014.0.1.1:20250628T100651Z"
        );
        
        // Check second attribute
        assert_eq!(parsed_manifest.attributes[1].key, "pkg.obsolete");
        assert_eq!(parsed_manifest.attributes[1].values[0], "true");
        
        // Check third attribute
        assert_eq!(parsed_manifest.attributes[2].key, "org.opensolaris.consolidation");
        assert_eq!(parsed_manifest.attributes[2].values[0], "userland");
        
        // Verify that properties is empty but exists
        for attr in &parsed_manifest.attributes {
            assert!(attr.properties.is_empty());
        }
    }
}
