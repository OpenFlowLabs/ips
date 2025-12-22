use anyhow::{Result, bail};
use semver::Version;
use std::collections::HashMap;
use url::Url;

use crate::Makefile;

#[derive(Debug, Clone)]
pub struct Component {
    pub version: Version,
    pub revision: Option<i32>,
    pub sources: HashMap<String, Source>,
    pub options: Vec<ComponentOption>,
    pub build_style: BuildStyle,
}

#[derive(Debug, Clone)]
pub enum BuildStyle {
    Configure,
    Cmake,
    Make,
    Meson,
    Custom,
}

#[derive(Debug, Clone)]
pub struct ComponentOption {
    pub platform: ComponentOptionPlatform,
    pub opt: String,
}

#[derive(Debug, Clone)]
pub enum ComponentOptionPlatform {
    All,
    Intel,
    Sparc,
    Arm,
}

#[derive(Debug, Clone)]
pub enum Source {
    GitSource {
        url: Url,
    },
    ArchiveSource {
        version: Version,
        hash: Option<String>,
        url: Url,
    },
}

impl Component {
    pub fn new_from_makefile(m: &Makefile) -> Result<Self> {
        let opts = if let Some(opts) = m.variables.get("CONFIGURE_OPTIONS") {
            opts.values
                .clone()
                .into_iter()
                .map(|v| ComponentOption {
                    platform: ComponentOptionPlatform::All,
                    opt: v,
                })
                .collect()
        } else {
            vec![]
        };

        let ver = semver::Version::parse(
            &m.get_first_value_of_variable_by_name("COMPONENT_VERSION")
                .ok_or_else(|| anyhow::anyhow!("missing component version"))?,
        )?;

        let revision = m
            .get_first_value_of_variable_by_name("COMPONENT_REVISION")
            .map(|s| s.parse().unwrap_or(0));

        let component_src = m.get_first_value_of_variable_by_name("COMPONENT_SRC");
        let component_archive_url = m.get_first_value_of_variable_by_name("COMPONENT_ARCHIVE_URL");
        let git_url = m.get_first_value_of_variable_by_name("GIT_URL");
        let component_archive_hash =
            m.get_first_value_of_variable_by_name("COMPONENT_ARCHIVE_HASH");
        let build_style = if let Some(style) = m.get_first_value_of_variable_by_name("BUILD_STYLE")
        {
            let s = match style.as_str() {
                "justmake" => BuildStyle::Make,
                "cmake" => BuildStyle::Cmake,
                "meson" => BuildStyle::Meson,
                "custom" => BuildStyle::Custom,
                "configure" => BuildStyle::Configure,
                _ => BuildStyle::Configure,
            };
            //TODO: Custom build style variable checks
            // something like guess_buildstyle_from_options
            s
        } else {
            BuildStyle::Configure
        };

        let src = if let (Some(name), Some(url)) = (component_src.clone(), component_archive_url) {
            (
                name,
                Source::ArchiveSource {
                    version: ver.clone(),
                    hash: component_archive_hash,
                    url: url.parse()?,
                },
            )
        } else if let (Some(name), Some(url)) = (component_src, git_url) {
            (name, Source::GitSource { url: url.parse()? })
        } else {
            bail!("no source found in makefile")
        };

        Ok(Self {
            version: ver,
            revision,
            sources: HashMap::from([src]),
            options: opts,
            build_style,
        })
    }
}
