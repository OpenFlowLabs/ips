use crate::sources::{Source, SourceError};
use libips::actions::{ActionError, File as FileAction, Manifest};
use std::collections::HashMap;
use std::env;
use std::env::{current_dir, set_current_dir};
use std::fs::{File, create_dir_all};
use std::io::Error as IOError;
use std::io::copy;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::result::Result as StdResult;
use thiserror::Error;

type Result<T> = StdResult<T, WorkspaceError>;

static DEFAULTWORKSPACEROOT: &str = "~/.ports/wks";
static DEFAULTARCH: &str = "i386";
static DEFAULTTAR: &str = "gtar";
static DEFAULTSHEBANG: &[u8; 19usize] = b"#!/usr/bin/env bash";

#[derive(Debug, Error)]
pub enum WorkspaceError {
    #[error("command returned {command} exit code: {code}")]
    NonZeroCommandExitCode { command: String, code: i32 },
    #[error("source {0} cannot be extracted")]
    UnextractableSource(Source),
    #[error("status code invalid")]
    InvalidStatusCode,
    #[error("source {0} has no extension")]
    SourceHasNoExtension(Source),
    #[error("io error: {0}")]
    IOError(#[from] IOError),
    #[error("could not lookup variable {0}")]
    VariableLookupError(String),
    #[error("source error: {0}")]
    SourceError(#[from] SourceError),
    #[error("reqwest error: {0}")]
    ReqwestError(#[from] reqwest::Error),
    #[error("unrunable script {0}")]
    UnrunableScript(String),
    #[error("path lookup error: {0}")]
    PathLookupError(#[from] which::Error),
    #[error("ips action error: {0}")]
    IpsActionError(#[from] ActionError),
}

pub struct Workspace {
    root: PathBuf,
    source_dir: PathBuf,
    build_dir: PathBuf,
    proto_dir: PathBuf,
}

fn init_root(ws: &Workspace) -> Result<()> {
    create_dir_all(&ws.root)?;
    create_dir_all(&ws.build_dir)?;
    create_dir_all(&ws.source_dir)?;
    create_dir_all(&ws.proto_dir)?;

    Ok(())
}

impl Workspace {
    pub fn new(root: &str) -> Result<Workspace> {
        let root_dir = if root.is_empty() {
            DEFAULTWORKSPACEROOT
        } else {
            root
        };

        let expanded_root_dir = shellexpand::full(root_dir)
            .map_err(|e| WorkspaceError::VariableLookupError(format!("{}", e.cause)))?
            .to_string();

        let ws = Workspace {
            root: Path::new(&expanded_root_dir).to_path_buf(),
            build_dir: Path::new(&expanded_root_dir)
                .join("build")
                .join(DEFAULTARCH),
            source_dir: Path::new(&expanded_root_dir).join("sources"),
            proto_dir: Path::new(&expanded_root_dir).join("build").join("proto"),
        };

        init_root(&ws)?;

        Ok(ws)
    }

    #[allow(dead_code)]
    pub fn expand_source_path(&self, fname: &str) -> PathBuf {
        self.source_dir.join(fname)
    }

    #[allow(dead_code)]
    pub fn get_proto_dir(&self) -> PathBuf {
        self.proto_dir.clone()
    }

    #[allow(dead_code)]
    pub fn get_build_dir(&self) -> PathBuf {
        self.build_dir.clone()
    }

    pub fn get_macros(&self) -> HashMap<String, PathBuf> {
        [
            ("proto_dir".to_owned(), self.proto_dir.clone()),
            ("build_dir".to_owned(), self.build_dir.clone()),
            ("source_dir".to_owned(), self.source_dir.clone()),
        ]
        .into()
    }

    pub fn get_sources(&self, sources: Vec<String>) -> Result<Vec<Source>> {
        let mut src_vec: Vec<Source> = vec![];
        for src in sources {
            let src_struct = Source::new(&src, &self.source_dir)?;
            let bytes = reqwest::blocking::get(src_struct.url.as_str())?.bytes()?;
            let mut out = File::create(&src_struct.local_name)?;
            copy(&mut bytes.as_ref(), &mut out)?;

            src_vec.push(src_struct);
        }

        Ok(src_vec)
    }

    pub fn unpack_all_sources(&self, sources: Vec<Source>) -> Result<()> {
        for src in sources {
            self.unpack_source(&src)?;
        }

        Ok(())
    }

    pub fn unpack_source(&self, src: &Source) -> Result<()> {
        match Path::new(&src.local_name).extension() {
            Some(ext) => {
                if !ext
                    .to_str()
                    .ok_or(WorkspaceError::SourceHasNoExtension(src.clone()))?
                    .contains("tar")
                {
                    return Err(WorkspaceError::UnextractableSource(src.clone()));
                }
                //TODO support inspecting the tar file to see if we have a top level directory or not
                let mut tar_cmd = Command::new(DEFAULTTAR)
                    .args([
                        "-C",
                        self.build_dir
                            .to_str()
                            .ok_or(WorkspaceError::UnextractableSource(src.clone()))?,
                        "-xaf",
                        src.local_name
                            .to_str()
                            .ok_or(WorkspaceError::UnextractableSource(src.clone()))?,
                        "--strip-components=1",
                    ])
                    .spawn()?;

                let status = tar_cmd.wait()?;
                if !status.success() {
                    return Err(WorkspaceError::NonZeroCommandExitCode {
                        command: "tar".to_owned(),
                        code: status.code().ok_or(WorkspaceError::InvalidStatusCode)?,
                    })?;
                }
            }
            None => {
                return Err(WorkspaceError::UnextractableSource(src.clone()));
            }
        }

        Ok(())
    }

    pub fn build(&self, build_script: String) -> Result<()> {
        let build_script_path = self.build_dir.join("build_script.sh");
        let mut file = File::create(&build_script_path)?;
        file.write_all(DEFAULTSHEBANG)?;
        file.write_all(b"\n")?;
        file.write_all(build_script.as_bytes())?;
        file.write_all(b"\n")?;
        let bash = which::which("bash")?;
        let filtered_env: HashMap<String, String> = env::vars()
            .filter(|(k, _)| k == "TERM" || k == "TZ" || k == "LANG" || k == "PATH")
            .collect();

        let mut shell = Command::new(bash)
            .args([
                "-ex",
                build_script_path
                    .to_str()
                    .ok_or(WorkspaceError::UnrunableScript("build_script".into()))?,
            ])
            .env_clear()
            .envs(&filtered_env)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()?;

        let status = shell.wait()?;
        if !status.success() {
            return Err(WorkspaceError::NonZeroCommandExitCode {
                command: "build_script".to_owned(),
                code: status.code().ok_or(WorkspaceError::InvalidStatusCode)?,
            })?;
        }

        Ok(())
    }

    pub fn package(&self, file_list: Vec<String>) -> Result<()> {
        let mut manifest = Manifest::default();
        let cwd = current_dir()?;
        set_current_dir(Path::new(&self.proto_dir))?;
        for f in file_list {
            if f.starts_with('/') {
                let mut f_mut = f.clone();
                f_mut.remove(0);
                manifest.add_file(FileAction::read_from_path(Path::new(&f_mut))?)
            } else {
                manifest.add_file(FileAction::read_from_path(Path::new(&f))?)
            }
        }
        set_current_dir(cwd)?;

        println!("{:?}", manifest);

        Ok(())
    }
}
