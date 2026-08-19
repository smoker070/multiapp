//! Profile lifecycle: create, list, launch, stop.
use crate::{launch, paths, proc, Error};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub app: String,
    pub name: String,
    pub running: bool,
    /// false when processes existed that we could not inspect (Windows elevation); destructive
    /// operations must refuse rather than assume the profile is idle.
    pub certain: bool,
    pub data_dir: PathBuf,
}

pub fn create(app: &str, name: &str) -> Result<PathBuf, Error> {
    paths::validate_name(name)?;
    let dir = paths::profile_data_dir(app, name)?;
    let profile_dir = dir.parent().ok_or(Error::NoHome)?.to_path_buf();
    if profile_dir.exists() {
        return Err(Error::Exists(format!("{app}/{name}")));
    }
    std::fs::create_dir_all(&dir)?;
    Ok(profile_dir)
}

pub fn list() -> Result<Vec<Profile>, Error> {
    let root = paths::profiles_root()?;
    let mut out = Vec::new();
    if !root.exists() {
        return Ok(out);
    }
    for app_ent in std::fs::read_dir(&root)? {
        let app_ent = app_ent?;
        if !app_ent.file_type()?.is_dir() {
            continue;
        }
        let app = app_ent.file_name().to_string_lossy().to_string();
        if app.starts_with('.') {
            continue; // .DS_Store and friends are not apps
        }
        for p_ent in std::fs::read_dir(app_ent.path())? {
            let p_ent = p_ent?;
            if !p_ent.file_type()?.is_dir() {
                continue;
            }
            let name = p_ent.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let data_dir = p_ent.path().join("data");
            let s = proc::sweep(&data_dir);
            out.push(Profile {
                app: app.clone(),
                name,
                running: s.is_running(),
                certain: s.is_certain(),
                data_dir,
            });
        }
    }
    out.sort_by(|a, b| (a.app.as_str(), a.name.as_str()).cmp(&(b.app.as_str(), b.name.as_str())));
    Ok(out)
}

pub fn launch_profile(app_ref: &str, app_key: &str, name: &str, extra: &[String]) -> Result<(), Error> {
    paths::validate_name(name)?;
    let dir = paths::profile_data_dir(app_key, name)?;
    let app = launch::resolve_app(app_ref)?;
    launch::launch(&app, &dir, extra)
}

pub fn stop(app_key: &str, name: &str, timeout_secs: u64) -> Result<bool, Error> {
    paths::validate_name(name)?;
    let dir = paths::profile_data_dir(app_key, name)?;
    if !dir.exists() {
        return Err(Error::NoProfile(format!("{app_key}/{name}")));
    }
    proc::quit_and_wait(&dir, std::time::Duration::from_secs(timeout_secs))
}
