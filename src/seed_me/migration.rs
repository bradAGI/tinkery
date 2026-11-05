//! Session-root migration and retired-storage refusal; preserves session bytes and Think records.

use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

/// Rename the default sessions folder once without reading or rewriting its contents.
/// Existing destinations and linked roots refuse; Think records are never opened or changed.
/// When `allow_move` is false, a pending move refuses before any filesystem writes.
pub fn migrate(home: Option<&OsStr>, allow_move: bool) -> Result<Option<String>, String> {
    let Some(home) = home.map(Path::new).filter(|p| p.is_absolute()) else {
        return Ok(None);
    };
    let old = home.join(".local/share/seed-me/sessions");
    let new = home.join(".local/share/tinkery/sessions");
    let metadata = match fs::symlink_metadata(&old) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("Cannot inspect the old sessions folder. Check its permissions before reopening Tinkery.".into()),
    };
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("The old sessions root is not an ordinary folder. Move it aside yourself before reopening Tinkery.".into());
    }
    match fs::symlink_metadata(&new) {
        Ok(_) => return Err("Both old and new sessions roots exist. Tinkery will not merge or read the old root. Move the leftover old root aside before reopening.".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
        Err(_) => return Err("Cannot inspect the new sessions folder. Check its permissions before reopening Tinkery.".into()),
    }
    if !allow_move {
        return Err("Snapshot mode does not move saved sessions. Open Tinkery normally to migrate them, then retry --snapshot.".into());
    }
    fs::create_dir_all(new.parent().unwrap())
        .map_err(|_| "Cannot create Tinkery's data folder. Check its permissions.")?;
    let old = old
        .canonicalize()
        .map_err(|_| "Cannot locate the old sessions folder.")?;
    let new = new
        .parent()
        .unwrap()
        .canonicalize()
        .map_err(|_| "Cannot locate Tinkery's data folder.")?
        .join("sessions");
    fs::rename(&old, &new).map_err(|_| "Cannot move saved sessions. Keep both locations unchanged and check permissions before retrying.")?;
    Ok(Some(
        "Moved saved sessions to ~/.local/share/tinkery/sessions.".into(),
    ))
}

/// Refuse reads and creation in retired storage, including aliases and new child destinations.
/// Resolve links in filesystem order; unreadable links and loops refuse.
pub(crate) fn refuse_old_root(path: &Path) -> Result<(), String> {
    if let Some(home) = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|_| "Cannot resolve the session path.")?
                .join(path)
        };
        let resolved = resolve_existing(&absolute, 40)?;
        if resolve_existing(&home.join(".local/share/seed-me/sessions"), 40)
            .is_ok_and(|retired| resolved.starts_with(retired))
        {
            return Err("The old sessions root is retired. Open the session under ~/.local/share/tinkery/sessions instead.".into());
        }
    }
    Ok(())
}

// Missing ordinary components may become new folders. Resolve each existing prefix before `..`.
fn resolve_existing(path: &Path, links_left: usize) -> Result<PathBuf, String> {
    let mut resolved = PathBuf::new();
    for component in path.components() {
        if component == std::path::Component::ParentDir {
            resolved.pop();
            continue;
        }
        resolved.push(component);
        match fs::symlink_metadata(&resolved) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                if links_left == 0 {
                    return Err("Cannot resolve the session path: too many symbolic links.".into());
                }
                let target = fs::read_link(&resolved)
                    .map_err(|_| "Cannot read the session path's symbolic link.")?;
                resolved.pop();
                resolved = resolve_existing(&resolved.join(target), links_left - 1)?;
            }
            Ok(_) => {
                resolved = resolved.canonicalize().map_err(
                    |_| "Cannot resolve the session path. Check its links and permissions.",
                )?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => {
                return Err(
                    "Cannot resolve the session path. Check its links and permissions.".into(),
                );
            }
        }
    }
    Ok(resolved)
}
