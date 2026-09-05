//! Backing an application's data up to a zip, and putting it back.
//!
//! Two rules govern everything here, and both were learned the hard way.
//!
//! **A restore MERGES; it never replaces.** An archive written without caches is an *incomplete*
//! copy of the directories it covers, so replacing a directory with it deletes everything it
//! skipped. On macOS that took a Telegram data directory from 18,918 files to 521. Anything a
//! restore does overwrite is copied into Multiapp's Trash first.
//!
//! **An archive is a credential.** The session files inside are the login for as long as the key
//! that decrypts them is present, so the manifest says so and the UI repeats it.
use crate::{appdata, paths, Error};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, Write};
use std::path::{Component, Path, PathBuf};

pub const MANIFEST: &str = "multiapp-manifest.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub kind: String,
    pub app: String,
    /// Paths as they were, relative to the user's home directory.
    pub paths: Vec<String>,
    pub excluded_cache: bool,
    pub created: String,
    pub os: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub files: usize,
    pub bytes: u64,
    pub path: PathBuf,
    /// Where displaced files were staged, when a restore overwrote anything.
    pub staged: Option<PathBuf>,
}

fn home() -> Result<PathBuf, Error> {
    directories::BaseDirs::new().map(|b| b.home_dir().to_path_buf()).ok_or(Error::NoHome)
}

/// Write `dirs` into a zip at `out`.
///
/// `sessions_only` narrows it to the files that carry the login — kilobytes rather than gigabytes,
/// which is what makes "save my login before reinstalling" practical at all.
pub fn create(
    app: &str,
    dirs: &[PathBuf],
    out: &Path,
    sessions_only: bool,
    include_cache: bool,
    now: &str,
) -> Result<Summary, Error> {
    let home = home()?;
    let f = std::fs::File::create(out)?;
    let mut zip = zip::ZipWriter::new(f);
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut files = 0usize;
    let mut bytes = 0u64;
    let mut rels: Vec<String> = Vec::new();

    for dir in dirs {
        let wanted: Vec<PathBuf> = if sessions_only {
            appdata::session_files(dir)
        } else {
            vec![dir.clone()]
        };
        for w in wanted {
            add_path(&mut zip, &opts, &home, &w, include_cache, &mut files, &mut bytes)?;
            if let Ok(r) = w.strip_prefix(&home) {
                rels.push(r.to_string_lossy().replace('\\', "/"));
            }
        }
    }

    let manifest = Manifest {
        kind: if sessions_only { "session".into() } else { "appdata".into() },
        app: app.to_string(),
        paths: rels,
        excluded_cache: !include_cache,
        created: now.to_string(),
        os: std::env::consts::OS.to_string(),
    };
    zip.start_file(MANIFEST, opts)?;
    zip.write_all(serde_json::to_string_pretty(&manifest).unwrap_or_default().as_bytes())?;
    zip.finish()?;
    Ok(Summary { files, bytes, path: out.to_path_buf(), staged: None })
}

fn add_path<W: Write + Seek>(
    zip: &mut zip::ZipWriter<W>,
    opts: &zip::write::FileOptions<'_, ()>,
    home: &Path,
    p: &Path,
    include_cache: bool,
    files: &mut usize,
    bytes: &mut u64,
) -> Result<(), Error> {
    let Ok(rel) = p.strip_prefix(home) else { return Ok(()) };
    let name = rel.to_string_lossy().replace('\\', "/");
    let meta = match std::fs::symlink_metadata(p) {
        Ok(m) => m,
        Err(_) => return Ok(()),
    };
    // Symlinks are skipped, never followed: following one can pull an entire home directory into an
    // archive, and no session state depends on them.
    if meta.file_type().is_symlink() {
        return Ok(());
    }
    if meta.is_file() {
        zip.start_file(name, *opts)?;
        let mut src = std::fs::File::open(p)?;
        let n = std::io::copy(&mut src, zip)?;
        *files += 1;
        *bytes += n;
        return Ok(());
    }
    if meta.is_dir() {
        let base = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
        if !include_cache && appdata::is_cache(base) {
            return Ok(());
        }
        for e in std::fs::read_dir(p)?.flatten() {
            add_path(zip, opts, home, &e.path(), include_cache, files, bytes)?;
        }
    }
    Ok(())
}

pub fn read_manifest(zip_path: &Path) -> Result<Manifest, Error> {
    let f = std::fs::File::open(zip_path)?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| Error::LaunchFailed(e.to_string()))?;
    let mut m = z.by_name(MANIFEST).map_err(|_| Error::LaunchFailed("not a Multiapp archive".into()))?;
    let mut s = String::new();
    m.read_to_string(&mut s)?;
    serde_json::from_str(&s).map_err(|e| Error::LaunchFailed(e.to_string()))
}

/// Is this entry name safe to join onto the home directory?
///
/// A zip can name `../../.ssh/authorized_keys` or `/etc/passwd`; joining either would write outside
/// the home directory. Both forms are refused rather than sanitised, because an archive containing
/// them is not one we wrote.
fn safe_relative(name: &str) -> Option<PathBuf> {
    let p = PathBuf::from(name);
    if p.is_absolute() {
        return None;
    }
    if p.components().any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }
    Some(p)
}

/// Extract an archive back into the home directory, MERGING into whatever is there.
pub fn restore(zip_path: &Path, stamp: &str) -> Result<Summary, Error> {
    let home = home()?;
    let manifest = read_manifest(zip_path)?;
    let f = std::fs::File::open(zip_path)?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| Error::LaunchFailed(e.to_string()))?;

    let trash = paths::trash_root()?.join(format!("restore-{}-{}", manifest.app, stamp));
    let mut staged_any = false;
    let mut files = 0usize;
    let mut bytes = 0u64;

    for i in 0..z.len() {
        let mut e = z.by_index(i).map_err(|e| Error::LaunchFailed(e.to_string()))?;
        let name = e.name().to_string();
        if name == MANIFEST || name.ends_with('/') {
            continue;
        }
        let Some(rel) = safe_relative(&name) else {
            return Err(Error::OutsideRoot(name)); // refuse the whole archive, not just the entry
        };
        let dest = home.join(&rel);

        // keep whatever is being overwritten, so a restore is itself undoable
        if dest.exists() {
            let keep = trash.join(&rel);
            if let Some(d) = keep.parent() {
                std::fs::create_dir_all(d)?;
            }
            let _ = std::fs::copy(&dest, &keep);
            staged_any = true;
        }
        if let Some(d) = dest.parent() {
            std::fs::create_dir_all(d)?;
        }
        let mut out = std::fs::File::create(&dest)?;
        bytes += std::io::copy(&mut e, &mut out)?;
        files += 1;
    }
    Ok(Summary {
        files,
        bytes,
        path: zip_path.to_path_buf(),
        staged: staged_any.then_some(trash),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// These tests move HOME, so they take the same process-wide lock as everything else that does.
    fn with_home<T>(f: impl FnOnce(&Path) -> T) -> T {
        let _g = paths::env_guard();
        let base = std::env::temp_dir().join(format!("ma-arch-{}-{:?}", std::process::id(), std::thread::current().id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let real = base.canonicalize().unwrap();
        let old = std::env::var("HOME").ok();
        std::env::set_var("HOME", &real);
        std::env::set_var("MULTIAPP_HOME", real.join("ma-root"));
        let out = f(&real);
        if let Some(h) = old { std::env::set_var("HOME", h); }
        let _ = std::fs::remove_dir_all(&base);
        out
    }

    fn write(p: &Path, s: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, s).unwrap();
    }

    #[test]
    fn round_trip_restores_the_files() {
        with_home(|home| {
            let dir = home.join("AppX");
            write(&dir.join("Cookies"), "session-value");
            write(&dir.join("sub/Local State"), "state");
            let zipf = home.join("out.zip");
            let s = create("AppX", std::slice::from_ref(&dir), &zipf, false, false, "now").unwrap();
            assert_eq!(s.files, 2);

            std::fs::remove_dir_all(&dir).unwrap();
            let r = restore(&zipf, "t").unwrap();
            assert_eq!(r.files, 2);
            assert_eq!(std::fs::read_to_string(dir.join("Cookies")).unwrap(), "session-value");
            assert_eq!(std::fs::read_to_string(dir.join("sub/Local State")).unwrap(), "state");
        })
    }

    /// The Telegram lesson, as a test: an archive written without caches is an INCOMPLETE copy, so a
    /// restore must never remove what the archive did not contain. Replacing took a real data
    /// directory from 18,918 files to 521.
    #[test]
    fn restore_merges_and_never_deletes_what_the_archive_omitted() {
        with_home(|home| {
            let dir = home.join("AppY");
            write(&dir.join("Cookies"), "v1");
            write(&dir.join("Cache/blob"), "big cache file");
            let zipf = home.join("y.zip");
            let s = create("AppY", std::slice::from_ref(&dir), &zipf, false, false, "now").unwrap();
            assert_eq!(s.files, 1, "the cache must be excluded from the archive");

            write(&dir.join("Cookies"), "v2-newer");
            write(&dir.join("UserNotes.txt"), "something the archive never saw");
            restore(&zipf, "t").unwrap();

            assert_eq!(std::fs::read_to_string(dir.join("Cookies")).unwrap(), "v1", "archived file restored");
            assert!(dir.join("Cache/blob").exists(), "an excluded cache must survive the restore");
            assert!(dir.join("UserNotes.txt").exists(), "a file the archive never held must survive");
        })
    }

    #[test]
    fn an_overwritten_file_is_kept_in_trash() {
        with_home(|home| {
            let dir = home.join("AppZ");
            write(&dir.join("Cookies"), "original");
            let zipf = home.join("z.zip");
            create("AppZ", std::slice::from_ref(&dir), &zipf, false, false, "now").unwrap();
            write(&dir.join("Cookies"), "replaced-by-user");
            let r = restore(&zipf, "t").unwrap();
            let staged = r.staged.expect("overwriting must stage the old file");
            let kept = std::fs::read_to_string(staged.join("AppZ").join("Cookies")).unwrap();
            assert_eq!(kept, "replaced-by-user", "the version that was overwritten must be recoverable");
        })
    }

    /// A zip may name `../../.ssh/authorized_keys`. Joining that onto HOME writes outside it, so the
    /// whole archive is refused rather than the entry silently skipped.
    #[test]
    fn a_traversing_entry_is_refused() {
        use std::io::Write as _;
        with_home(|home| {
            let zipf = home.join("evil.zip");
            {
                let f = std::fs::File::create(&zipf).unwrap();
                let mut z = zip::ZipWriter::new(f);
                let o: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
                z.start_file(MANIFEST, o).unwrap();
                z.write_all(br#"{"kind":"appdata","app":"E","paths":[],"excluded_cache":true,"created":"n","os":"x"}"#).unwrap();
                z.start_file("../escaped.txt", o).unwrap();
                z.write_all(b"pwned").unwrap();
                z.finish().unwrap();
            }
            let err = restore(&zipf, "t").unwrap_err();
            assert!(matches!(err, Error::OutsideRoot(_)), "got {err:?}");
            assert!(!home.parent().unwrap().join("escaped.txt").exists(), "nothing may be written outside home");
        })
    }

    #[test]
    fn sessions_only_takes_the_session_files_and_not_the_bulk() {
        with_home(|home| {
            let dir = home.join("AppS");
            write(&dir.join("Cookies"), "c");
            write(&dir.join("Local Storage/leveldb/000.ldb"), "ls");
            write(&dir.join("History"), "x".repeat(5000).as_str());
            write(&dir.join("Cache/huge"), "y".repeat(9000).as_str());
            let zipf = home.join("s.zip");
            let s = create("AppS", &[dir], &zipf, true, false, "now").unwrap();
            // Cookies + the leveldb file; History and Cache are neither session nor wanted
            assert_eq!(s.files, 2, "session backup must not sweep the whole directory");
        })
    }
}
