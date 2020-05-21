//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

mod actions;

#[macro_use] extern crate failure;

#[cfg(test)]
mod tests {

    use crate::actions::{Manifest, Property, Dir};
    use crate::actions::{parse_manifest_string, Attr};
    use std::collections::HashSet;

    #[test]
    fn parse_attributes() {
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
        set name=pkg.summary value=\\\"provided mouse accessibility enhancements\\\"
        set name=info.upstream value=X.Org Foundation
        set name=pkg.description value=Latvian language support's extra files
        set name=variant.arch value=i386 optional=testing optionalWithString=\"test ing\"
        set name=info.source-url value=http://www.pgpool.net/download.php?f=pgpool-II-3.3.1.tar.gz
        set name=pkg.summary value=\\\"'XZ Utils - loss-less file compression application and library.'\\\"");

        let mut optional_hash = HashSet::new();
        optional_hash.insert(Property{key: String::from("optional"), value:String::from("testing")});
        optional_hash.insert(Property{key: String::from("optionalWithString"), value:String::from("test ing")});

        let test_results = vec![
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
                key: String::from("pkg.summary"),
                values: vec![String::from("provided mouse accessibility enhancements")],
                properties: HashSet::new(),
            },
            Attr{
                key: String::from("info.upstream"),
                values: vec![String::from("X.Org Foundation")],
                properties: HashSet::new(),
            },
            Attr{
                key: String::from("pkg.description"),
                values: vec![String::from("Latvian language support's extra files")],
                properties: HashSet::new(),
            },
            Attr{
                key: String::from("variant.arch"),
                values: vec![String::from("i386")],
                properties: optional_hash,
            },
            Attr{
                key: String::from("info.source-url"),
                values: vec![String::from("http://www.pgpool.net/download.php?f=pgpool-II-3.3.1.tar.gz")],
                properties: HashSet::new(),
            },
            Attr{
                key: String::from("pkg.summary"),
                values: vec![String::from("'XZ Utils - loss-less file compression application and library.'")], //TODO knock out the single quotes
                properties: HashSet::new(),
            }
        ];

        let mut manifest = Manifest::new();
        match parse_manifest_string(manifest_string) {
            Ok(m) => manifest = m,
            Err(e) => {
                println!("{}", e);
                assert!(false, "caught error");
            }
        };

        assert_eq!(manifest.attributes.len(), 17);

        for (pos, attr) in manifest.attributes.iter().enumerate() {
            assert_eq!(attr.key, test_results[pos].key);

            for (vpos, val) in attr.values.iter().enumerate() {
                assert_eq!(val, &test_results[pos].values[vpos]);
            }
        }
    }

    #[test]
    fn parse_direcory_actions() {
        let manifest_string = String::from("dir group=bin mode=0755 owner=root path=etc/nginx
        dir group=bin mode=0755 owner=root path=usr/share/nginx
        dir group=bin mode=0755 owner=root path=usr/share/nginx/html
        dir group=bin mode=0755 owner=webservd path=var/nginx/logs
        dir group=bin mode=0755 owner=root path=\"var/nginx\"");

        let test_results = vec![
            Dir{
                group: String::from("bin"),
                mode: String::from("0755"),
                owner: String::from("root"),
                path: String::from("etc/nginx"),
                ..Dir::default()
            },Dir{
                group: String::from("bin"),
                mode: String::from("0755"),
                owner: String::from("root"),
                path: String::from("usr/share/nginx"),
                ..Dir::default()
            },Dir{
                group: String::from("bin"),
                mode: String::from("0755"),
                owner: String::from("root"),
                path: String::from("usr/share/nginx/html"),
                ..Dir::default()
            },Dir{
                group: String::from("bin"),
                mode: String::from("0755"),
                owner: String::from("webservd"),
                path: String::from("var/nginx/logs"),
                ..Dir::default()
            },Dir{
                group: String::from("bin"),
                mode: String::from("0755"),
                owner: String::from("root"),
                path: String::from("var/nginx"),
                ..Dir::default()
            },
        ];

        let mut manifest = Manifest::new();
        match parse_manifest_string(manifest_string) {
            Ok(m) => manifest = m,
            Err(e) => {
                println!("{}", e);
                assert!(false, "caught error");
            }
        };

        assert_eq!(manifest.directories.len(), test_results.len());

        for (pos, attr) in manifest.directories.iter().enumerate() {
            assert_eq!(attr.group, test_results[pos].group);
            assert_eq!(attr.mode, test_results[pos].mode);
            assert_eq!(attr.owner, test_results[pos].owner);
            assert_eq!(attr.path, test_results[pos].path);

            //for (vpos, val) in attr.facets.iter().enumerate() {
            //    assert_eq!(val, &test_results[pos].facets.);
            //}
        }
    }
}
