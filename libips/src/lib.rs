//  This Source Code Form is subject to the terms of
//  the Mozilla Public License, v. 2.0. If a copy of the
//  MPL was not distributed with this file, You can
//  obtain one at https://mozilla.org/MPL/2.0/.

#[allow(clippy::result_large_err)]
pub mod actions;
pub mod digest;
pub mod fmri;
pub mod image;
pub mod payload;
pub mod repository;
pub mod publisher;
pub mod transformer;
pub mod solver;
pub mod depend;
pub mod api;
mod test_json_manifest;

#[cfg(test)]
mod publisher_tests;

#[cfg(test)]
mod tests {

    use crate::actions::Attr;
    use crate::actions::{Dependency, Dir, Facet, File, Link, Manifest, Property};
    use crate::digest::{Digest, DigestAlgorithm, DigestSource};
    use crate::fmri::Fmri;
    use crate::payload::Payload;
    use std::collections::HashMap;

    use maplit::hashmap;

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
set name=pkg.summary value=\"provided mouse accessibility enhancements\"
set name=info.upstream value=\"X.Org Foundation\"
set name=pkg.description value=\"Latvian language support's extra files\"
set name=variant.arch value=i386 optional=testing optionalWithString=\"test ing\"
set name=info.source-url value=\"http://www.pgpool.net/download.php?f=pgpool-II-3.3.1.tar.gz\"
set name=pkg.summary value=\"'XZ Utils - loss-less file compression application and library.'\"");

        let mut optional_hash = HashMap::new();
        optional_hash.insert(
            String::from("optional"),
            Property {
                key: String::from("optional"),
                value: String::from("testing"),
            },
        );
        optional_hash.insert(
            String::from("optionalWithString"),
            Property {
                key: String::from("optionalWithString"),
                value: String::from("test ing"),
            },
        );

        let test_results = vec![
            Attr{
                key: String::from("pkg.fmri"),
                values: vec![String::from("pkg://openindiana.org/web/server/nginx@1.18.0,5.11-2020.0.1.0:20200421T195136Z")],
                properties: HashMap::new(),
            },
            Attr{
                key: String::from("com.oracle.info.name"),
                values: vec![String::from("nginx"), String::from("test")],
                properties: HashMap::new(),
            },
            Attr{
                key: String::from("userland.info.git-remote"),
                values: vec![String::from("git://github.com/OpenIndiana/oi-userland.git")],
                properties: HashMap::new(),
            },
            Attr{
                key: String::from("userland.info.git-branch"),
                values: vec![String::from("HEAD")],
                properties: HashMap::new(),
            },
            Attr{
                key: String::from("userland.info.git-rev"),
                values: vec![String::from("1665491ba61bd494bf73e2916cd2250f3024260e")],
                properties: HashMap::new(),
            },
            Attr{
                key: String::from("pkg.summary"),
                values: vec![String::from("Nginx Webserver")],
                properties: HashMap::new(),
            },
            Attr{
                key: String::from("info.classification"),
                values: vec![String::from("org.opensolaris.category.2008:Web Services/Application and Web Servers")],
                properties: HashMap::new(),
            },
            Attr{
                key: String::from("info.upstream-url"),
                values: vec![String::from("http://nginx.net/")],
                properties: HashMap::new(),
            },
            Attr{
                key: String::from("info.source-url"),
                values: vec![String::from("http://nginx.org/download/nginx-1.18.0.tar.gz")],
                properties: HashMap::new(),
            },
            Attr{
                key: String::from("org.opensolaris.consolidation"),
                values: vec![String::from("userland")],
                properties: HashMap::new(),
            },
            Attr{
                key: String::from("com.oracle.info.version"),
                values: vec![String::from("1.18.0")],
                properties: HashMap::new(),
            },
            Attr{
                key: String::from("pkg.summary"),
                values: vec![String::from("provided mouse accessibility enhancements")],
                properties: HashMap::new(),
            },
            Attr{
                key: String::from("info.upstream"),
                values: vec![String::from("X.Org Foundation")],
                properties: HashMap::new(),
            },
            Attr{
                key: String::from("pkg.description"),
                values: vec![String::from("Latvian language support's extra files")],
                properties: HashMap::new(),
            },
            Attr{
                key: String::from("variant.arch"),
                values: vec![String::from("i386")],
                properties: optional_hash,
            },
            Attr{
                key: String::from("info.source-url"),
                values: vec![String::from("http://www.pgpool.net/download.php?f=pgpool-II-3.3.1.tar.gz")],
                properties: HashMap::new(),
            },
            Attr{
                key: String::from("pkg.summary"),
                values: vec![String::from("'XZ Utils - loss-less file compression application and library.'")], //TODO knock out the single quotes
                properties: HashMap::new(),
            }
        ];

        let res = Manifest::parse_string(manifest_string);
        assert!(res.is_ok(), "error during Manifest parsing: {:?}", res);
        let manifest = res.unwrap();

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
        let manifest_string = String::from(
            "dir group=bin mode=0755 owner=root path=etc/nginx
dir group=bin mode=0755 owner=root path=usr/share/nginx
dir group=bin mode=0755 owner=root path=usr/share/nginx/html
dir group=bin mode=0755 owner=webservd path=var/nginx/logs
dir group=bin mode=0755 owner=root path=\"var/nginx\"",
        );

        let test_results = vec![
            Dir {
                group: String::from("bin"),
                mode: String::from("0755"),
                owner: String::from("root"),
                path: String::from("etc/nginx"),
                ..Dir::default()
            },
            Dir {
                group: String::from("bin"),
                mode: String::from("0755"),
                owner: String::from("root"),
                path: String::from("usr/share/nginx"),
                ..Dir::default()
            },
            Dir {
                group: String::from("bin"),
                mode: String::from("0755"),
                owner: String::from("root"),
                path: String::from("usr/share/nginx/html"),
                ..Dir::default()
            },
            Dir {
                group: String::from("bin"),
                mode: String::from("0755"),
                owner: String::from("webservd"),
                path: String::from("var/nginx/logs"),
                ..Dir::default()
            },
            Dir {
                group: String::from("bin"),
                mode: String::from("0755"),
                owner: String::from("root"),
                path: String::from("var/nginx"),
                ..Dir::default()
            },
        ];

        let res = Manifest::parse_string(manifest_string);
        assert!(res.is_ok(), "error during Manifest parsing: {:?}", res);
        let manifest = res.unwrap();

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

    #[test]
    fn parse_file_actions() {
        let manifest_string = String::from("file 4b76e83bb4bb7c87176b72ef805fe78ecae60d2c chash=7288afc78233791bb8e13b3e13aa4f0b4b1d6ee8 group=bin mode=555 owner=root path=lib/svc/method/http-nginx pkg.content-hash=file:sha512t_256:42007aaee6bd54977eb33f91db28f931ab11c39787ba9f7851b6baf0d142185b pkg.content-hash=gzip:sha512t_256:ec144533fa077af1d5b152d8c7549f113902021d71808adb12ea3f92bda9fd66 pkg.csize=975 pkg.size=1855
file 72e0496a02e72e7380b0b62cdc8410108302876f chash=2f82b51db9cbba0705cb680e5aa0f11ff237009b group=sys mode=0444 owner=root path=lib/svc/manifest/network/http-nginx.xml pkg.content-hash=file:sha512t_256:c0c3640d6e61b53a3dc4228adff7532ec6b5d09bf1847991a3aaa5eb3e04d19a pkg.content-hash=gzip:sha512t_256:e1999bae58ef887d81dc686b794429a9dea0e7674b631c2a08f07fb9b34440e2 pkg.csize=1067 pkg.size=2844 restart_fmri=svc:/system/manifest-import:default
file 95de71d58b37f9f74bede0e91bc381d6059fc2d7 chash=c2e2e4cf82ec527800a2170d9e2565b75d557012 group=bin mode=0444 owner=root path=usr/share/nginx/html/50x.html pkg.content-hash=file:sha512t_256:b592728ea1dcd6dd0924e1e6767e217ad70ec6973086911d8bc07d44695b9f0e pkg.content-hash=gzip:sha512t_256:8407d82b497c4a865841ab8874207cc5a4d581ba574d66074ef5f92f05ee13cf pkg.csize=327 pkg.size=494
file 7dd71afcfb14e105e80b0c0d7fce370a28a41f0a chash=50b7bcf6c555b8e9bde1eacd2c3d5c34a757c312 group=bin mode=0444 owner=root path=usr/share/nginx/html/index.html pkg.content-hash=file:sha512t_256:204038cd5fbbcdd2c3d24acb7f41b1e861c51d689f53202ec69b43bdba01cb60 pkg.content-hash=gzip:sha512t_256:34bad6066578cf03289b0c957cb4f01a9353f91b3b95079d69bf9e12dd569279 pkg.csize=381 pkg.size=612
file cbf596ddb3433a8e0d325f3c188bec9c1bb746b3 chash=2df27ca83841b9c8e38c5aa30760372773166928 group=bin mode=0644 owner=root path=etc/nginx/fastcgi.conf pkg.content-hash=file:sha512t_256:d260c064680ec58135d9a290ed3cfd64274db769701ab3df2bfdeb653a864518 pkg.content-hash=gzip:sha512t_256:4924c0f4bdc37b832afd281ad07b0bf339c8c3a0e2d95e076998d46fab76a084 pkg.csize=448 pkg.size=1077 preserve=true
file da38e2a0dded838afbe0eade6cb837ac30fd8046 chash=530616dc345f6acf0aea26db06e56aa41b2f510d group=bin mode=0644 owner=root path=etc/nginx/fastcgi_params pkg.content-hash=file:sha512t_256:baeeb2df301f8764568a86884c127e90faf39bee4ff0e53fb4a890955e605cee pkg.content-hash=gzip:sha512t_256:5c6f541692556eacbde4ea1536de3c1af2cd8e9980fc4edca36851a97ed671ba pkg.csize=430 pkg.size=1007 preserve=true
file 407cb51b397ba4ad90a2246640a81af18e2e917a chash=00d285c15dd65f24c4c89d5790094c38432a1ac6 group=bin mode=0644 owner=root path=etc/nginx/koi-utf pkg.content-hash=file:sha512t_256:06381b2c4a28fe88c0d908f1cd81453c9482358c8195163e294b8def8924b366 pkg.content-hash=gzip:sha512t_256:d66022b08971eaf9ddf3230a991b0d8352fcefe0f797305a94b5ca0574d70ff5 pkg.csize=938 pkg.size=2837 preserve=true
file 19ec7fb71e7f00d7e8a1cfc1013490f0cfee572b chash=0f2588ac25780698ea7ebeac3ea0e9041502d501 group=bin mode=0644 owner=root path=etc/nginx/koi-win pkg.content-hash=file:sha512t_256:92d4df1df754d3e2cd8c52aba7415680c86097803b437bf0edcd8d022ab6aa8c pkg.content-hash=gzip:sha512t_256:2ad3bb0540d800f2115691c96e8ed35b9b91eb5c248bea199da22ffd102cc847 pkg.csize=749 pkg.size=2223 preserve=true
file e39dbc36680b717ec902fadc805a302f1cf62245 chash=325af5a4b735284a3cdfd3b04bd249ff22334965 group=bin mode=0644 owner=root path=etc/nginx/mime.types pkg.content-hash=file:sha512t_256:8217c6955d644400707c4ecf1539ece4ee2fd1be4838654860f2ef2ecacdebd4 pkg.content-hash=gzip:sha512t_256:46566d205da4d67a6e12a1d3d2f78e3602770ce42ef2c117ee95b821aec90100 pkg.csize=990 pkg.size=5231 preserve=true
file d143ca7a6aac765d28724af54d969a4bd2202383 chash=adacb374c514459417f07cacd4f8bf60644c9651 group=bin mode=0644 owner=root path=etc/nginx/nginx.conf pkg.content-hash=file:sha512t_256:cc9263a836b4db441340d2e041adf10136c9a8aa31259b868000f88c84032ba1 pkg.content-hash=gzip:sha512t_256:cf0cd12b5f3f1d9d15378e1a1bacaaff7589bf2c129312b277b66ea3418acc54 pkg.csize=997 pkg.size=2798 preserve=true
file 379c1e2a2a5ffb8c91a07328d4c9be2bc58799fd chash=2c75c59e0de9208a9b96460d0566e5686708310c group=bin mode=0644 owner=root path=etc/nginx/scgi_params pkg.content-hash=file:sha512t_256:e6dd7076b6319abc3fcd04554fede95c8cc40f1e21a83772c36577f939e81cb6 pkg.content-hash=gzip:sha512t_256:48efb28df3607f1a8b67eab95d4ca19526e8351d10529d97cb4af05250f8ee95 pkg.csize=275 pkg.size=636 preserve=true
file cc2fcdb4605dcac23d59f667889ccbdfdc6e3668 chash=62320c6c207a26bf9c68c39d0372f4d4b97b905f group=bin mode=0644 owner=root path=etc/nginx/uwsgi_params pkg.content-hash=file:sha512t_256:eb133ae0a357df02b4b02615bc47dc2e5328105dac2dbcbd647667e9bbc3b2fd pkg.content-hash=gzip:sha512t_256:e5a2625a67f5502c5911d7e7a850030b6af89929e182b2da74ecf6e79df0e9d2 pkg.csize=284 pkg.size=664 preserve=true
file e10f2d42c9e581901d810928d01a3bf8f3372838 chash=fd231cdd1a726fcb2abeba90b31cbf4c7df6df4d group=bin mode=0644 owner=root path=etc/nginx/win-utf pkg.content-hash=file:sha512t_256:7620f21db4c06f3eb863c0cb0a8b3f62c435abd2f8f47794c42f08ad434d90dd pkg.content-hash=gzip:sha512t_256:ca16a95ddd6ef2043969db20915935829b8ccb6134588e1710b24baf45afd7bb pkg.csize=1197 pkg.size=3610 preserve=true
file 6d5f820bb1d67594c7b757c79ef6f9242df49e98 chash=3ab17dde089f1eac7abd37d8efd700b5139d70b2 elfarch=i386 elfbits=64 elfhash=25b0cdd7736cddad78ce91b61385a8fdde91f7b2 group=bin mode=0555 owner=root path=usr/sbin/nginx pkg.content-hash=gelf:sha512t_256:add9bfb171c2a173b8f12d375884711527f40e592d100a337a9fae078c8beabd pkg.content-hash=gelf.unsigned:sha512t_256:add9bfb171c2a173b8f12d375884711527f40e592d100a337a9fae078c8beabd pkg.content-hash=file:sha512t_256:3d87b058a8e69b3a8dfab142f5e856549dcd531a371e3ca4d2be391655b0d076 pkg.content-hash=gzip:sha512t_256:7f93c48194b3e164ea35a9d2ddff310215769dbd27b45e9ab72beef1cce0d4f6 pkg.csize=657230 pkg.size=1598048
file path=usr/lib/golang/1.16/src/cmd/go/testdata/mod/rsc.io_!c!g!o_v1.0.0.txt
file path=usr/lib/golang/1.16/src/runtime/runtime-gdb_test.go.~1~");

        let test_results = vec![
            File {
                payload: Some(Payload {
                    primary_identifier: Digest {
                        hash: String::from("4b76e83bb4bb7c87176b72ef805fe78ecae60d2c"),
                        ..Digest::default()
                    },
                    additional_identifiers: vec![
                        Digest {
                            hash: "7288afc78233791bb8e13b3e13aa4f0b4b1d6ee8".to_string(),
                            ..Digest::default()
                        },
                        Digest {
                            hash:
                                "42007aaee6bd54977eb33f91db28f931ab11c39787ba9f7851b6baf0d142185b"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::UncompressedFile,
                        },
                        Digest {
                            hash:
                                "ec144533fa077af1d5b152d8c7549f113902021d71808adb12ea3f92bda9fd66"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::GzipCompressed,
                        },
                    ],
                    ..Payload::default()
                }),
                group: "bin".to_string(),
                mode: "555".to_string(),
                owner: "root".to_string(),
                path: "lib/svc/method/http-nginx".to_string(),
                properties: vec![
                    Property {
                        key: "pkg.csize".to_string(),
                        value: "975".to_string(),
                    },
                    Property {
                        key: "pkg.size".to_string(),
                        value: "1855".to_string(),
                    },
                ],
                ..File::default()
            },
            File {
                payload: Some(Payload {
                    primary_identifier: Digest {
                        hash: String::from("72e0496a02e72e7380b0b62cdc8410108302876f"),
                        ..Digest::default()
                    },
                    additional_identifiers: vec![
                        Digest {
                            hash: "2f82b51db9cbba0705cb680e5aa0f11ff237009b".to_string(),
                            ..Digest::default()
                        },
                        Digest {
                            hash:
                                "c0c3640d6e61b53a3dc4228adff7532ec6b5d09bf1847991a3aaa5eb3e04d19a"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::UncompressedFile,
                        },
                        Digest {
                            hash:
                                "e1999bae58ef887d81dc686b794429a9dea0e7674b631c2a08f07fb9b34440e2"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::GzipCompressed,
                        },
                    ],
                    ..Payload::default()
                }),
                group: "sys".to_string(),
                mode: "0444".to_string(),
                owner: "root".to_string(),
                path: "lib/svc/manifest/network/http-nginx.xml".to_string(),
                properties: vec![
                    Property {
                        key: "pkg.csize".to_string(),
                        value: "1067".to_string(),
                    },
                    Property {
                        key: "pkg.size".to_string(),
                        value: "2844".to_string(),
                    },
                    Property {
                        key: "restart_fmri".to_string(),
                        value: "svc:/system/manifest-import:default".to_string(),
                    },
                ],
                ..File::default()
            },
            File {
                payload: Some(Payload {
                    primary_identifier: Digest {
                        hash: String::from("95de71d58b37f9f74bede0e91bc381d6059fc2d7"),
                        ..Digest::default()
                    },
                    additional_identifiers: vec![
                        Digest {
                            hash: "c2e2e4cf82ec527800a2170d9e2565b75d557012".to_string(),
                            ..Digest::default()
                        },
                        Digest {
                            hash:
                                "b592728ea1dcd6dd0924e1e6767e217ad70ec6973086911d8bc07d44695b9f0e"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::UncompressedFile,
                        },
                        Digest {
                            hash:
                                "8407d82b497c4a865841ab8874207cc5a4d581ba574d66074ef5f92f05ee13cf"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::GzipCompressed,
                        },
                    ],
                    ..Payload::default()
                }),
                group: "bin".to_string(),
                mode: "0444".to_string(),
                owner: "root".to_string(),
                path: "usr/share/nginx/html/50x.html".to_string(),
                properties: vec![
                    Property {
                        key: "pkg.csize".to_string(),
                        value: "327".to_string(),
                    },
                    Property {
                        key: "pkg.size".to_string(),
                        value: "494".to_string(),
                    },
                ],
                ..File::default()
            },
            File {
                payload: Some(Payload {
                    primary_identifier: Digest {
                        hash: String::from("7dd71afcfb14e105e80b0c0d7fce370a28a41f0a"),
                        ..Digest::default()
                    },
                    additional_identifiers: vec![
                        Digest {
                            hash: "50b7bcf6c555b8e9bde1eacd2c3d5c34a757c312".to_string(),
                            ..Digest::default()
                        },
                        Digest {
                            hash:
                                "204038cd5fbbcdd2c3d24acb7f41b1e861c51d689f53202ec69b43bdba01cb60"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::UncompressedFile,
                        },
                        Digest {
                            hash:
                                "34bad6066578cf03289b0c957cb4f01a9353f91b3b95079d69bf9e12dd569279"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::GzipCompressed,
                        },
                    ],
                    ..Payload::default()
                }),
                group: "bin".to_string(),
                mode: "0444".to_string(),
                owner: "root".to_string(),
                path: "usr/share/nginx/html/index.html".to_string(),
                properties: vec![
                    Property {
                        key: "pkg.csize".to_string(),
                        value: "381".to_string(),
                    },
                    Property {
                        key: "pkg.size".to_string(),
                        value: "612".to_string(),
                    },
                ],
                ..File::default()
            },
            File {
                payload: Some(Payload {
                    primary_identifier: Digest {
                        hash: String::from("cbf596ddb3433a8e0d325f3c188bec9c1bb746b3"),
                        ..Digest::default()
                    },
                    additional_identifiers: vec![
                        Digest {
                            hash: "2df27ca83841b9c8e38c5aa30760372773166928".to_string(),
                            ..Digest::default()
                        },
                        Digest {
                            hash:
                                "d260c064680ec58135d9a290ed3cfd64274db769701ab3df2bfdeb653a864518"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::UncompressedFile,
                        },
                        Digest {
                            hash:
                                "4924c0f4bdc37b832afd281ad07b0bf339c8c3a0e2d95e076998d46fab76a084"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::GzipCompressed,
                        },
                    ],
                    ..Payload::default()
                }),
                group: "bin".to_string(),
                mode: "0644".to_string(),
                owner: "root".to_string(),
                path: "etc/nginx/fastcgi.conf".to_string(),
                preserve: true,
                properties: vec![
                    Property {
                        key: "pkg.csize".to_string(),
                        value: "448".to_string(),
                    },
                    Property {
                        key: "pkg.size".to_string(),
                        value: "1077".to_string(),
                    },
                ],
                ..File::default()
            },
            File {
                payload: Some(Payload {
                    primary_identifier: Digest {
                        hash: String::from("da38e2a0dded838afbe0eade6cb837ac30fd8046"),
                        ..Digest::default()
                    },
                    additional_identifiers: vec![
                        Digest {
                            hash: "530616dc345f6acf0aea26db06e56aa41b2f510d".to_string(),
                            ..Digest::default()
                        },
                        Digest {
                            hash:
                                "baeeb2df301f8764568a86884c127e90faf39bee4ff0e53fb4a890955e605cee"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::UncompressedFile,
                        },
                        Digest {
                            hash:
                                "5c6f541692556eacbde4ea1536de3c1af2cd8e9980fc4edca36851a97ed671ba"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::GzipCompressed,
                        },
                    ],
                    ..Payload::default()
                }),
                group: "bin".to_string(),
                mode: "0644".to_string(),
                owner: "root".to_string(),
                path: "etc/nginx/fastcgi_params".to_string(),
                preserve: true,
                properties: vec![
                    Property {
                        key: "pkg.csize".to_string(),
                        value: "430".to_string(),
                    },
                    Property {
                        key: "pkg.size".to_string(),
                        value: "1007".to_string(),
                    },
                ],
                ..File::default()
            },
            File {
                payload: Some(Payload {
                    primary_identifier: Digest {
                        hash: String::from("407cb51b397ba4ad90a2246640a81af18e2e917a"),
                        ..Digest::default()
                    },
                    additional_identifiers: vec![
                        Digest {
                            hash: "00d285c15dd65f24c4c89d5790094c38432a1ac6".to_string(),
                            ..Digest::default()
                        },
                        Digest {
                            hash:
                                "06381b2c4a28fe88c0d908f1cd81453c9482358c8195163e294b8def8924b366"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::UncompressedFile,
                        },
                        Digest {
                            hash:
                                "d66022b08971eaf9ddf3230a991b0d8352fcefe0f797305a94b5ca0574d70ff5"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::GzipCompressed,
                        },
                    ],
                    ..Payload::default()
                }),
                group: "bin".to_string(),
                mode: "0644".to_string(),
                owner: "root".to_string(),
                path: "etc/nginx/koi-utf".to_string(),
                preserve: true,
                properties: vec![
                    Property {
                        key: "pkg.csize".to_string(),
                        value: "938".to_string(),
                    },
                    Property {
                        key: "pkg.size".to_string(),
                        value: "2837".to_string(),
                    },
                ],
                ..File::default()
            },
            File {
                payload: Some(Payload {
                    primary_identifier: Digest {
                        hash: String::from("19ec7fb71e7f00d7e8a1cfc1013490f0cfee572b"),
                        ..Digest::default()
                    },
                    additional_identifiers: vec![
                        Digest {
                            hash: "0f2588ac25780698ea7ebeac3ea0e9041502d501".to_string(),
                            ..Digest::default()
                        },
                        Digest {
                            hash:
                                "92d4df1df754d3e2cd8c52aba7415680c86097803b437bf0edcd8d022ab6aa8c"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::UncompressedFile,
                        },
                        Digest {
                            hash:
                                "2ad3bb0540d800f2115691c96e8ed35b9b91eb5c248bea199da22ffd102cc847"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::GzipCompressed,
                        },
                    ],
                    ..Payload::default()
                }),
                group: "bin".to_string(),
                mode: "0644".to_string(),
                owner: "root".to_string(),
                path: "etc/nginx/koi-win".to_string(),
                preserve: true,
                properties: vec![
                    Property {
                        key: "pkg.csize".to_string(),
                        value: "749".to_string(),
                    },
                    Property {
                        key: "pkg.size".to_string(),
                        value: "2223".to_string(),
                    },
                ],
                ..File::default()
            },
            File {
                payload: Some(Payload {
                    primary_identifier: Digest {
                        hash: String::from("e39dbc36680b717ec902fadc805a302f1cf62245"),
                        ..Digest::default()
                    },
                    additional_identifiers: vec![
                        Digest {
                            hash: "325af5a4b735284a3cdfd3b04bd249ff22334965".to_string(),
                            ..Digest::default()
                        },
                        Digest {
                            hash:
                                "8217c6955d644400707c4ecf1539ece4ee2fd1be4838654860f2ef2ecacdebd4"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::UncompressedFile,
                        },
                        Digest {
                            hash:
                                "46566d205da4d67a6e12a1d3d2f78e3602770ce42ef2c117ee95b821aec90100"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::GzipCompressed,
                        },
                    ],
                    ..Payload::default()
                }),
                group: "bin".to_string(),
                mode: "0644".to_string(),
                owner: "root".to_string(),
                path: "etc/nginx/mime.types".to_string(),
                preserve: true,
                properties: vec![
                    Property {
                        key: "pkg.csize".to_string(),
                        value: "990".to_string(),
                    },
                    Property {
                        key: "pkg.size".to_string(),
                        value: "5231".to_string(),
                    },
                ],
                ..File::default()
            },
            File {
                payload: Some(Payload {
                    primary_identifier: Digest {
                        hash: String::from("d143ca7a6aac765d28724af54d969a4bd2202383"),
                        ..Digest::default()
                    },
                    additional_identifiers: vec![
                        Digest {
                            hash: "adacb374c514459417f07cacd4f8bf60644c9651".to_string(),
                            ..Digest::default()
                        },
                        Digest {
                            hash:
                                "cc9263a836b4db441340d2e041adf10136c9a8aa31259b868000f88c84032ba1"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::UncompressedFile,
                        },
                        Digest {
                            hash:
                                "cf0cd12b5f3f1d9d15378e1a1bacaaff7589bf2c129312b277b66ea3418acc54"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::GzipCompressed,
                        },
                    ],
                    ..Payload::default()
                }),
                group: "bin".to_string(),
                mode: "0644".to_string(),
                owner: "root".to_string(),
                path: "etc/nginx/nginx.conf".to_string(),
                preserve: true,
                properties: vec![
                    Property {
                        key: "pkg.csize".to_string(),
                        value: "997".to_string(),
                    },
                    Property {
                        key: "pkg.size".to_string(),
                        value: "2798".to_string(),
                    },
                ],
                ..File::default()
            },
            File {
                payload: Some(Payload {
                    primary_identifier: Digest {
                        hash: String::from("379c1e2a2a5ffb8c91a07328d4c9be2bc58799fd"),
                        ..Digest::default()
                    },
                    additional_identifiers: vec![
                        Digest {
                            hash: "2c75c59e0de9208a9b96460d0566e5686708310c".to_string(),
                            ..Digest::default()
                        },
                        Digest {
                            hash:
                                "e6dd7076b6319abc3fcd04554fede95c8cc40f1e21a83772c36577f939e81cb6"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::UncompressedFile,
                        },
                        Digest {
                            hash:
                                "48efb28df3607f1a8b67eab95d4ca19526e8351d10529d97cb4af05250f8ee95"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::GzipCompressed,
                        },
                    ],
                    ..Payload::default()
                }),
                group: "bin".to_string(),
                mode: "0644".to_string(),
                owner: "root".to_string(),
                path: "etc/nginx/scgi_params".to_string(),
                preserve: true,
                properties: vec![
                    Property {
                        key: "pkg.csize".to_string(),
                        value: "275".to_string(),
                    },
                    Property {
                        key: "pkg.size".to_string(),
                        value: "636".to_string(),
                    },
                ],
                ..File::default()
            },
            File {
                payload: Some(Payload {
                    primary_identifier: Digest {
                        hash: String::from("cc2fcdb4605dcac23d59f667889ccbdfdc6e3668"),
                        ..Digest::default()
                    },
                    additional_identifiers: vec![
                        Digest {
                            hash: "62320c6c207a26bf9c68c39d0372f4d4b97b905f".to_string(),
                            ..Digest::default()
                        },
                        Digest {
                            hash:
                                "eb133ae0a357df02b4b02615bc47dc2e5328105dac2dbcbd647667e9bbc3b2fd"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::UncompressedFile,
                        },
                        Digest {
                            hash:
                                "e5a2625a67f5502c5911d7e7a850030b6af89929e182b2da74ecf6e79df0e9d2"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::GzipCompressed,
                        },
                    ],
                    ..Payload::default()
                }),
                group: "bin".to_string(),
                mode: "0644".to_string(),
                owner: "root".to_string(),
                path: "etc/nginx/uwsgi_params".to_string(),
                preserve: true,
                properties: vec![
                    Property {
                        key: "pkg.csize".to_string(),
                        value: "284".to_string(),
                    },
                    Property {
                        key: "pkg.size".to_string(),
                        value: "664".to_string(),
                    },
                ],
                ..File::default()
            },
            File {
                payload: Some(Payload {
                    primary_identifier: Digest {
                        hash: String::from("e10f2d42c9e581901d810928d01a3bf8f3372838"),
                        ..Digest::default()
                    },
                    additional_identifiers: vec![
                        Digest {
                            hash: "fd231cdd1a726fcb2abeba90b31cbf4c7df6df4d".to_string(),
                            ..Digest::default()
                        },
                        Digest {
                            hash:
                                "7620f21db4c06f3eb863c0cb0a8b3f62c435abd2f8f47794c42f08ad434d90dd"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::UncompressedFile,
                        },
                        Digest {
                            hash:
                                "ca16a95ddd6ef2043969db20915935829b8ccb6134588e1710b24baf45afd7bb"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::GzipCompressed,
                        },
                    ],
                    ..Payload::default()
                }),
                group: "bin".to_string(),
                mode: "0644".to_string(),
                owner: "root".to_string(),
                path: "etc/nginx/win-utf".to_string(),
                preserve: true,
                properties: vec![
                    Property {
                        key: "pkg.csize".to_string(),
                        value: "1197".to_string(),
                    },
                    Property {
                        key: "pkg.size".to_string(),
                        value: "3610".to_string(),
                    },
                ],
                ..File::default()
            },
            File {
                payload: Some(Payload {
                    primary_identifier: Digest {
                        hash: String::from("6d5f820bb1d67594c7b757c79ef6f9242df49e98"),
                        ..Digest::default()
                    },
                    additional_identifiers: vec![
                        Digest {
                            hash: "3ab17dde089f1eac7abd37d8efd700b5139d70b2".to_string(),
                            ..Digest::default()
                        },
                        Digest {
                            hash:
                                "add9bfb171c2a173b8f12d375884711527f40e592d100a337a9fae078c8beabd"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::GNUElf,
                        },
                        Digest {
                            hash:
                                "add9bfb171c2a173b8f12d375884711527f40e592d100a337a9fae078c8beabd"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::GNUElfUnsigned,
                        },
                        Digest {
                            hash:
                                "3d87b058a8e69b3a8dfab142f5e856549dcd531a371e3ca4d2be391655b0d076"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::UncompressedFile,
                        },
                        Digest {
                            hash:
                                "7f93c48194b3e164ea35a9d2ddff310215769dbd27b45e9ab72beef1cce0d4f6"
                                    .to_string(),
                            algorithm: DigestAlgorithm::SHA512Half,
                            source: DigestSource::GzipCompressed,
                        },
                    ],
                    ..Payload::default()
                }),
                group: "bin".to_string(),
                mode: "0555".to_string(),
                owner: "root".to_string(),
                path: "usr/sbin/nginx".to_string(),
                properties: vec![
                    Property {
                        key: "elfarch".to_string(),
                        value: "i386".to_string(),
                    },
                    Property {
                        key: "elfbits".to_string(),
                        value: "64".to_string(),
                    },
                    Property {
                        key: "elfhash".to_string(),
                        value: "25b0cdd7736cddad78ce91b61385a8fdde91f7b2".to_string(),
                    },
                    Property {
                        key: "pkg.csize".to_string(),
                        value: "657230".to_string(),
                    },
                    Property {
                        key: "pkg.size".to_string(),
                        value: "1598048".to_string(),
                    },
                ],
                ..File::default()
            },
            File {
                path: "usr/lib/golang/1.16/src/cmd/go/testdata/mod/rsc.io_!c!g!o_v1.0.0.txt".into(),
                ..File::default()
            },
            File {
                path: "usr/lib/golang/1.16/src/runtime/runtime-gdb_test.go.~1~".into(),
                ..File::default()
            },
        ];

        let res = Manifest::parse_string(manifest_string);
        assert!(res.is_ok(), "error during Manifest parsing: {:?}", res);
        let manifest = res.unwrap();

        assert_eq!(manifest.files.len(), test_results.len());

        for (pos, file) in manifest.files.iter().enumerate() {
            assert_eq!(file.group, test_results[pos].group);
            assert_eq!(file.mode, test_results[pos].mode);
            assert_eq!(file.owner, test_results[pos].owner);
            assert_eq!(file.path, test_results[pos].path);
            assert_eq!(file.preserve, test_results[pos].preserve);
            if let Some(payload_expected) = &test_results[pos].payload {
                assert_ne!(file.payload, None);
                assert_eq!(
                    file.payload.as_ref().unwrap().primary_identifier.hash,
                    payload_expected.primary_identifier.hash
                );
            }

            for (vpos, val) in file.properties.iter().enumerate() {
                assert_eq!(val.key, test_results[pos].properties[vpos].key);
                assert_eq!(val.value, test_results[pos].properties[vpos].value);
            }

            if let Some(payload) = &file.payload {
                for (vpos, val) in payload.additional_identifiers.iter().enumerate() {
                    assert_eq!(
                        val.hash,
                        test_results[pos]
                            .payload
                            .as_ref()
                            .unwrap()
                            .additional_identifiers[vpos]
                            .hash
                    );
                    assert_eq!(
                        val.source,
                        test_results[pos]
                            .payload
                            .as_ref()
                            .unwrap()
                            .additional_identifiers[vpos]
                            .source
                    );
                    assert_eq!(
                        val.algorithm,
                        test_results[pos]
                            .payload
                            .as_ref()
                            .unwrap()
                            .additional_identifiers[vpos]
                            .algorithm
                    );
                }
            }
        }
    }

    #[test]
    fn parse_dependency_actions() {
        let manifest_string = String::from("depend fmri=pkg:/system/library@0.5.11-2020.0.1.19563 type=require
depend fmri=pkg:/system/file-system/nfs@0.5.11,5.11-2020.0.1.19951 type=incorporate
depend facet.version-lock.system/data/hardware-registry=true fmri=pkg:/system/data/hardware-registry@2020.2.22,5.11-2020.0.1.19951 type=incorporate
depend facet.version-lock.xvm=true fmri=xvm@0.5.11-2015.0.2.0 type=incorporate
depend facet.version-lock.system/mozilla-nss=true fmri=system/mozilla-nss@3.51.1-2020.0.1.0 type=incorporate");

        let test_results = vec![
            Dependency {
                fmri: Some(Fmri::parse("pkg:/system/library@0.5.11-2020.0.1.19563").unwrap()),
                dependency_type: "require".to_string(),
                ..Dependency::default()
            },
            Dependency {
                fmri: Some(
                    Fmri::parse("pkg:/system/file-system/nfs@0.5.11,5.11-2020.0.1.19951").unwrap(),
                ),
                dependency_type: "incorporate".to_string(),
                ..Dependency::default()
            },
            Dependency {
                fmri: Some(
                    Fmri::parse("pkg:/system/data/hardware-registry@2020.2.22,5.11-2020.0.1.19951")
                        .unwrap(),
                ),
                dependency_type: "incorporate".to_string(),
                facets: hashmap! {
                    "version-lock.system/data/hardware-registry".to_string() => Facet{
                        name: "version-lock.system/data/hardware-registry".to_string(),
                        value: "true".to_string(),
                    }
                },
                ..Dependency::default()
            },
            Dependency {
                fmri: Some(Fmri::parse("xvm@0.5.11-2015.0.2.0").unwrap()),
                dependency_type: "incorporate".to_string(),
                facets: hashmap! {
                    "version-lock.xvm".to_string() => Facet{
                        name: "version-lock.xvm".to_string(),
                        value: "true".to_string(),
                    }
                },
                ..Dependency::default()
            },
            Dependency {
                fmri: Some(Fmri::parse("system/mozilla-nss@3.51.1-2020.0.1.0").unwrap()),
                dependency_type: "incorporate".to_string(),
                facets: hashmap! {
                    "version-lock.system/mozilla-nss".to_string() => Facet{
                        name: "version-lock.system/mozilla-nss".to_string(),
                        value: "true".to_string(),
                    }
                },
                ..Dependency::default()
            },
        ];

        let res = Manifest::parse_string(manifest_string);
        assert!(res.is_ok(), "error during Manifest parsing: {:?}", res);
        let manifest = res.unwrap();

        assert_eq!(manifest.dependencies.len(), test_results.len());
        for (pos, dependency) in manifest.dependencies.iter().enumerate() {
            // Compare the string representation of the FMRIs
            if let (Some(dep_fmri), Some(test_fmri)) = (&dependency.fmri, &test_results[pos].fmri) {
                assert_eq!(dep_fmri.to_string(), test_fmri.to_string());
            } else {
                assert_eq!(dependency.fmri.is_none(), test_results[pos].fmri.is_none());
            }

            assert_eq!(
                dependency.dependency_type,
                test_results[pos].dependency_type
            );
            for (_, (key, facet)) in dependency.facets.iter().enumerate() {
                let fres = test_results[pos].facets.get(key);
                assert!(
                    fres.is_some(),
                    "error no facet with name: {:?} found",
                    facet.name
                );
                let f = fres.unwrap();
                assert_eq!(facet.name, f.name);
                assert_eq!(facet.value, f.value);
            }
        }
    }

    #[test]
    fn parse_line_breaks() {
        let manifest_string = String::from(
            "link \
    path=usr/lib/cups/backend/http \
    target=ipp
file Solaris/desktop-print-management mode=0555 \
     path=usr/lib/cups/bin/desktop-print-management
file Solaris/desktop-print-management-applet mode=0555 \
     path=usr/lib/cups/bin/desktop-print-management-applet
file Solaris/smb mode=0555 \
     path=usr/lib/cups/backend/smb
# SMF service start method script
file Solaris/svc-cupsd mode=0644 path=lib/svc/method/svc-cupsd

# SMF help
file Solaris/ManageCUPS.html mode=0444 \
     path=usr/lib/help/auths/locale/C/ManageCUPS.html",
        );

        let file_results = vec![
            File {
                path: "usr/lib/cups/bin/desktop-print-management".to_string(),
                mode: "0555".to_string(),
                properties: vec![Property {
                    key: "original-path".to_string(),
                    value: "Solaris/desktop-print-management".to_string(),
                }],
                ..File::default()
            },
            File {
                path: "usr/lib/cups/bin/desktop-print-management-applet".to_string(),
                mode: "0555".to_string(),
                properties: vec![Property {
                    key: "original-path".to_string(),
                    value: "Solaris/desktop-print-management-applet".to_string(),
                }],
                ..File::default()
            },
            File {
                path: "usr/lib/cups/backend/smb".to_string(),
                mode: "0555".to_string(),
                properties: vec![Property {
                    key: "original-path".to_string(),
                    value: "Solaris/smb".to_string(),
                }],
                ..File::default()
            },
            File {
                path: "lib/svc/method/svc-cupsd".to_string(),
                mode: "0644".to_string(),
                properties: vec![Property {
                    key: "original-path".to_string(),
                    value: "Solaris/svc-cupsd".to_string(),
                }],
                ..File::default()
            },
            File {
                path: "usr/lib/help/auths/locale/C/ManageCUPS.html".to_string(),
                mode: "0444".to_string(),
                properties: vec![Property {
                    key: "original-path".to_string(),
                    value: "Solaris/ManageCUPS.html".to_string(),
                }],
                ..File::default()
            },
        ];

        let link_results = vec![Link {
            path: "usr/lib/cups/backend/http".to_string(),
            target: "ipp".to_string(),
            ..Link::default()
        }];

        let res = Manifest::parse_string(manifest_string);
        assert!(res.is_ok(), "error during Manifest parsing: {:?}", res);
        let manifest = res.unwrap();

        for (pos, file) in manifest.files.iter().enumerate() {
            assert_eq!(file.path, file_results[pos].path);
            assert_eq!(file.properties[0].key, file_results[pos].properties[0].key);
            assert_eq!(
                file.properties[0].value,
                file_results[pos].properties[0].value
            );
            assert_eq!(file.mode, file_results[pos].mode);
        }

        for (pos, link) in manifest.links.iter().enumerate() {
            assert_eq!(link.path, link_results[pos].path);
            assert_eq!(link.target, link_results[pos].target);
        }
    }

    #[test]
    fn parse_unicode() {
        let manifest_string = String::from(
            "link \
    path=usr/lib/cups/пертинах/http \
    target=Про
# SMF blub
link path=usr/lib/cups/пертинах/http target=blub",
        );

        let link_results = vec![
            Link {
                path: "usr/lib/cups/пертинах/http".to_string(),
                target: "Про".to_string(),
                ..Link::default()
            },
            Link {
                path: "usr/lib/cups/пертинах/http".to_string(),
                target: "blub".to_string(),
                ..Link::default()
            },
        ];

        let res = Manifest::parse_string(manifest_string);
        assert!(res.is_ok(), "error during Manifest parsing: {:?}", res);
        let manifest = res.unwrap();

        for (pos, link) in manifest.links.iter().enumerate() {
            assert_eq!(link.path, link_results[pos].path);
            assert_eq!(link.target, link_results[pos].target);
        }
    }
}
