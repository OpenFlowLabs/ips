mod actions;

#[cfg(test)]
mod tests {

    use crate::actions::ParseManifestString;
    use crate::actions::Manifest;
    use crate::actions::ManifestError;
    use std::error;
    use std::fmt;

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
        let mut manifest = Manifest::new(); 
        match ParseManifestString(manifest_string){
            Ok(m) => manifest = m,
            Err(_) => assert!(false, "caught error")
        };
        for attr in manifest.Attributes {
            println!("Name: {}", attr.Key);
            for val in attr.Values {
                println!("Value: {}", val)
            }
            println!();
        }
        //assert_eq!(manifest.Attributes.len(), 12);
    }
    
}

