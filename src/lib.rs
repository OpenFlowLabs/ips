//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

mod actions;

#[cfg(test)]
mod tests {

    use crate::actions::{parse_manifest_string, Attr};
    use crate::actions::Manifest;
    use crate::actions::ManifestError;
    use std::error;
    use std::fmt;
    use std::collections::HashSet;

    #[test]
    fn parse_manifest() {
        let manifest_string = String::from("set name=pkg.fmri value=pkg://openindiana.org/web/server/nginx@1.18.0,5.11-2020.0.1.0:20200421T195136Z
        set name=com.oracle.info.name value=nginx value=test
        set name=userland.info.git-remote value=git://github.com/OpenIndiana/oi-userland.git
        set name=userland.info.git-branch value=HEAD
        set name=userland.info.git-rev value=1665491ba61bd494bf73e2916cd2250f3024260e
        set name=pkg.summary value=\"Nginx Webserver\"
        set name=info.classification value=\"org.opensolaris.category.2008:Web Services/Application and Web Servers\"
        set name=info.upstream-url value=http://nginx.net/
        set name=info.source-url value=http://nginx.org/download/nginx-1.18.0.tar.gz
        set name=org.opensolaris.consolidation value=userland
        set name=com.oracle.info.version value=1.18.0
        set name=variant.arch value=i386");
        let testResults = vec![
            Attr{
                key: String::from("pkg.fmri"),
                values: vec![String::from("pkg://openindiana.org/web/server/nginx@1.18.0,5.11-2020.0.1.0:20200421T195136Z")],
                properties: HashSet::new(),
            },
            Attr{
                key: String::from("com.oracle.info.name"),
                values: vec![String::from("nginx"), String::from("test")],
                properties: HashSet::new(),
            },
            Attr{
                key: String::from("userland.info.git-remote"),
                values: vec![String::from("git://github.com/OpenIndiana/oi-userland.git")],
                properties: HashSet::new(),
            },
            Attr{
                key: String::from("userland.info.git-branch"),
                values: vec![String::from("HEAD")],
                properties: HashSet::new(),
            },
            Attr{
                key: String::from("userland.info.git-rev"),
                values: vec![String::from("1665491ba61bd494bf73e2916cd2250f3024260e")],
                properties: HashSet::new(),
            },
            Attr{
                key: String::from("pkg.summary"),
                values: vec![String::from("Nginx Webserver")],
                properties: HashSet::new(),
            },
            Attr{
                key: String::from("info.classification"),
                values: vec![String::from("org.opensolaris.category.2008:Web Services/Application and Web Servers")],
                properties: HashSet::new(),
            },
            Attr{
                key: String::from("info.upstream-url"),
                values: vec![String::from("http://nginx.net/")],
                properties: HashSet::new(),
            },
            Attr{
                key: String::from("info.source-url"),
                values: vec![String::from("http://nginx.org/download/nginx-1.18.0.tar.gz")],
                properties: HashSet::new(),
            },
            Attr{
                key: String::from("org.opensolaris.consolidation"),
                values: vec![String::from("userland")],
                properties: HashSet::new(),
            },
            Attr{
                key: String::from("com.oracle.info.version"),
                values: vec![String::from("1.18.0")],
                properties: HashSet::new(),
            },
            Attr{
                key: String::from("variant.arch"),
                values: vec![String::from("i386")],
                properties: HashSet::new(),
            }
        ];

        let mut manifest = Manifest::new(); 
        match parse_manifest_string(manifest_string){
            Ok(m) => manifest = m,
            Err(_) => assert!(false, "caught error")
        };
        assert_eq!(manifest.attributes.len(), 12);
        for (pos, attr) in manifest.attributes.iter().enumerate() {
            assert_eq!(attr.key, testResults[pos].key);
            for (vpos, val) in attr.values.iter().enumerate() {
                assert_eq!(val, &testResults[pos].values[vpos]);
            }
        }
    }
    
}

