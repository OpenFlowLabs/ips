//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;

    use tempfile::TempDir;

    use crate::publisher::PublisherClient;
    use crate::repository::file_backend::FileBackend;
    use crate::repository::{RepositoryVersion, WritableRepository};

    #[test]
    fn publisher_client_basic_flow() {
        // Create a temporary repository directory
        let tmp = TempDir::new().expect("tempdir");
        let repo_path = tmp.path().to_path_buf();

        // Initialize repository
        let mut backend =
            FileBackend::create(&repo_path, RepositoryVersion::V4).expect("create repo");
        backend.add_publisher("test").expect("add publisher");

        // Prepare a prototype directory with a nested file
        let proto_dir = repo_path.join("proto");
        let nested = proto_dir.join("nested").join("dir");
        fs::create_dir_all(&nested).expect("create proto dirs");
        let file_path = nested.join("hello.txt");
        let content = b"Hello PublisherClient!";
        let mut f = fs::File::create(&file_path).expect("create file");
        f.write_all(content).expect("write content");

        // Use PublisherClient to publish
        let mut client = PublisherClient::open(&repo_path, "test").expect("open client");
        client.open_transaction().expect("open tx");
        let manifest = client
            .build_manifest_from_dir(&proto_dir)
            .expect("build manifest");
        client.publish(manifest, true).expect("publish");

        // Verify the manifest exists at the default path for unknown version
        let manifest_path =
            FileBackend::construct_package_dir(&repo_path, "test", "unknown").join("manifest");
        assert!(
            manifest_path.exists(),
            "manifest not found at {}",
            manifest_path.display()
        );

        // Verify at least one file was stored under publisher/test/file
        let file_root = repo_path.join("publisher").join("test").join("file");
        assert!(
            file_root.exists(),
            "file store root does not exist: {}",
            file_root.display()
        );
        let mut any_file = false;
        if let Ok(entries) = fs::read_dir(&file_root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Ok(files) = fs::read_dir(&path) {
                        for f in files.flatten() {
                            if f.path().is_file() {
                                any_file = true;
                                break;
                            }
                        }
                    }
                } else if path.is_file() {
                    any_file = true;
                }
                if any_file {
                    break;
                }
            }
        }
        assert!(any_file, "no stored file found in file store");
    }
}

#[cfg(test)]
mod transform_rule_integration_tests {
    use crate::actions::Manifest;
    use crate::publisher::PublisherClient;
    use crate::repository::file_backend::FileBackend;
    use crate::repository::{RepositoryVersion, WritableRepository};
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn publisher_client_applies_transform_rules_from_file() {
        // Setup repository and publisher
        let tmp = TempDir::new().expect("tempdir");
        let repo_path = tmp.path().to_path_buf();
        let mut backend =
            FileBackend::create(&repo_path, RepositoryVersion::V4).expect("create repo");
        backend.add_publisher("test").expect("add publisher");

        // Prototype directory with a file
        let proto_dir = repo_path.join("proto2");
        fs::create_dir_all(&proto_dir).expect("mkdir proto2");
        let file_path = proto_dir.join("foo.txt");
        let mut f = fs::File::create(&file_path).expect("create file");
        f.write_all(b"data").expect("write");

        // Create a rules file that emits a pkg.summary attribute
        let rules_path = repo_path.join("rules.txt");
        let rules_text = "<transform file match_type=path pattern=.* operation=emit -> set name=pkg.summary value=\"Added via rules\">\n";
        fs::write(&rules_path, rules_text).expect("write rules");

        // Use PublisherClient to load rules, build manifest and publish
        let mut client = PublisherClient::open(&repo_path, "test").expect("open client");
        let loaded = client
            .load_transform_rules_from_file(&rules_path)
            .expect("load rules");
        assert!(loaded >= 1, "expected at least one rule loaded");
        client.open_transaction().expect("open tx");
        let manifest = client
            .build_manifest_from_dir(&proto_dir)
            .expect("build manifest");
        client.publish(manifest, false).expect("publish");

        // Read stored manifest and verify attribute
        let manifest_path =
            FileBackend::construct_package_dir(&repo_path, "test", "unknown").join("manifest");
        assert!(
            manifest_path.exists(),
            "manifest missing: {}",
            manifest_path.display()
        );
        let json = fs::read_to_string(&manifest_path).expect("read manifest");
        let parsed: Manifest = serde_json::from_str(&json).expect("parse manifest json");
        let has_summary = parsed
            .attributes
            .iter()
            .any(|a| a.key == "pkg.summary" && a.values.iter().any(|v| v == "Added via rules"));
        assert!(
            has_summary,
            "pkg.summary attribute added via rules not found"
        );
    }
}
