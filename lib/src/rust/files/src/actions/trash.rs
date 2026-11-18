use crate::preview::{checked_file, fd_path, version};
use gio::prelude::*;
use rustix::fs::{AtFlags, Mode, OFlags, RenameFlags, openat, renameat_with, unlinkat};
use std::ffi::OsString;
use std::fs::{self, File, Metadata};
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub(super) struct Item {
    files: File,
    info: File,
    source: File,
    record: File,
    name: OsString,
    record_name: OsString,
    source_version: String,
    record_version: String,
    parent: File,
}

fn at(dir: &File, name: &std::ffi::OsStr) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}", dir.as_raw_fd())).join(name)
}

pub(super) fn pin(
    path: &Path,
    expected: &str,
    allowed: &impl Fn(&Path) -> bool,
) -> Result<(File, File), String> {
    let parent = checked_file(
        path.parent().ok_or("Cannot trash filesystem root.")?,
        OFlags::DIRECTORY | OFlags::PATH,
        allowed,
    )?;
    let source = File::from(
        openat(
            &parent,
            path.file_name().ok_or("Invalid file name.")?,
            OFlags::PATH | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| e.to_string())?,
    );
    if version(&source.metadata().map_err(|e| e.to_string())?) != expected {
        return Err("File changed. Refresh and try again.".into());
    }
    Ok((parent, source))
}

fn same_file(a: &Metadata, b: &Metadata) -> bool {
    a.dev() == b.dev() && a.ino() == b.ino()
}

impl Item {
    pub(super) fn locate(
        root: &Path,
        path: &Path,
        parent: File,
        source: File,
        allowed: &impl Fn(&Path) -> bool,
    ) -> Option<Self> {
        let source_meta = source.metadata().ok()?;
        if !source_meta.is_dir() && source_meta.nlink() != 1 {
            return None;
        }
        let files = checked_file(
            &root.join("files"),
            OFlags::RDONLY | OFlags::DIRECTORY,
            allowed,
        )
        .ok()?;
        let info = checked_file(
            &root.join("info"),
            OFlags::RDONLY | OFlags::DIRECTORY,
            allowed,
        )
        .ok()?;
        if source_meta.dev() != files.metadata().ok()?.dev() {
            return None;
        }
        let entries = fs::read_dir(at(&files, ".".as_ref())).ok()?;
        let started = Instant::now();
        for entry in entries.take(4096).flatten() {
            if started.elapsed() > Duration::from_millis(90) {
                break;
            }
            let name = entry.file_name();
            let data = fs::symlink_metadata(entry.path()).ok();
            if !data.is_some_and(|data| same_file(&source_meta, &data)) {
                continue;
            }
            let mut record_name = name.clone();
            record_name.push(".trashinfo");
            let mut record = File::from(
                openat(
                    &info,
                    &record_name,
                    OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                    Mode::empty(),
                )
                .ok()?,
            );
            let record_meta = record.metadata().ok()?;
            if !record_meta.is_file() || record_meta.len() > 16384 {
                return None;
            }
            let mut text = String::new();
            (&mut record).take(16385).read_to_string(&mut text).ok()?;
            let mut lines = text.lines();
            if text.len() > 16384 || lines.next() != Some("[Trash Info]") {
                return None;
            }
            let mut paths = lines.filter_map(|s| s.strip_prefix("Path="));
            let encoded = paths.next()?;
            if paths.next().is_some()
                || !encoded.starts_with('/')
                || gio::File::for_uri(&format!("file://{encoded}"))
                    .path()?
                    .as_path()
                    != path
            {
                return None;
            }
            return Some(Self {
                files,
                info,
                source,
                record,
                name,
                record_name,
                source_version: version(&source_meta),
                record_version: version(&record_meta),
                parent,
            });
        }
        None
    }

    pub(super) fn restore(
        &self,
        path: &Path,
        allowed: &impl Fn(&Path) -> bool,
    ) -> Result<(), String> {
        let parent = fd_path(&self.parent)?;
        let expected_parent = path
            .parent()
            .ok_or("Invalid restore location.")?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if parent != expected_parent
            || !allowed(&parent)
            || !allowed(path)
            || !allowed(&fd_path(&self.files)?)
            || !allowed(&fd_path(&self.info)?)
        {
            return Err("Restore location changed or is excluded.".into());
        }
        for (dir, name, handle, expected) in [
            (&self.files, &self.name, &self.source, &self.source_version),
            (
                &self.info,
                &self.record_name,
                &self.record,
                &self.record_version,
            ),
        ] {
            let present = fs::symlink_metadata(at(dir, name)).map_err(|e| e.to_string())?;
            if &version(&present) != expected
                || !same_file(&handle.metadata().map_err(|e| e.to_string())?, &present)
            {
                return Err("Trash item changed. Restore it using the file manager.".into());
            }
        }
        let dest = path.file_name().ok_or("Invalid restore name.")?;
        renameat_with(
            &self.files,
            &self.name,
            &self.parent,
            dest,
            RenameFlags::NOREPLACE,
        )
        .map_err(|e| e.to_string())?;
        // The .trashinfo reserves this name against compliant trash writers until data has moved.
        if let Ok(present) = fs::symlink_metadata(at(&self.info, &self.record_name))
            && version(&present) == self.record_version
        {
            let _ = unlinkat(&self.info, &self.record_name, AtFlags::empty());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn restores_selected_identity_and_never_overwrites() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("Trash");
        let files = root.join("files");
        let info = root.join("info");
        fs::create_dir_all(&files).unwrap();
        fs::create_dir(&info).unwrap();
        let path = tmp.path().join("report #1");
        fs::write(&path, "selected").unwrap();
        let (parent, source) =
            pin(&path, &version(&fs::metadata(&path).unwrap()), &|_| true).unwrap();
        let record = format!(
            "[Trash Info]\nPath={}\nDeletionDate=2026-01-02T00:00:00\n",
            gio::File::for_path(&path)
                .uri()
                .strip_prefix("file://")
                .unwrap()
        );
        fs::write(files.join("report #1"), "older").unwrap();
        fs::write(info.join("report #1.trashinfo"), &record).unwrap();
        fs::rename(&path, files.join("report #1.2")).unwrap();
        fs::write(info.join("report #1.2.trashinfo"), &record).unwrap();
        let item = Item::locate(&root, &path, parent, source, &|_| true).unwrap();
        fs::write(&path, "replacement").unwrap();
        assert!(item.restore(&path, &|_| true).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"replacement");
        fs::remove_file(&path).unwrap();
        item.restore(&path, &|_| true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"selected");
        assert_eq!(fs::read(files.join("report #1")).unwrap(), b"older");
        assert!(info.join("report #1.trashinfo").is_file());
        assert!(!info.join("report #1.2.trashinfo").exists());
    }
}
