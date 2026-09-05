use thiserror::Error as ThisError;

#[derive(Debug, ThisError)]
pub enum Error {
    #[error("cannot determine your home directory")]
    NoHome,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("archive: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("app not found: {0}")]
    AppNotFound(String),
    #[error("launch failed: {0}")]
    LaunchFailed(String),
    #[error("invalid profile name '{0}' (letters, digits, spaces, _ and -; must start alphanumeric)")]
    BadName(String),
    #[error("refusing to touch '{0}': outside the Multiapp root")]
    OutsideRoot(String),
    #[error("profile '{0}' already exists")]
    Exists(String),
    #[error("no such profile: {0}")]
    NoProfile(String),
    #[error("'{0}' is running — stop it first")]
    Running(String),
    #[error("cannot be sure '{0}' is stopped: {1} process(es) could not be inspected")]
    Uncertain(String, usize),
}
