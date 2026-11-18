use std::fs::{self, File, Metadata};
use std::io::{self, BufReader, Cursor, Read, Seek, Write};
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use base64::{Engine, engine::general_purpose::STANDARD};
use image::{ImageFormat, ImageReader};
use rustix::fs::{CWD, Mode, OFlags, ResolveFlags, openat2};
use rustix::process::{Resource, Rlimit, setrlimit};
use serde_json::{Value, json};

const TEXT_LIMIT: u64 = 10 << 20;
const IMAGE_LIMIT: u64 = 50 << 20;
const PREFIX: usize = 8192;

pub fn version(m: &Metadata) -> String {
    format!(
        "{}:{}:{}:{}:{}:{}",
        m.dev(),
        m.ino(),
        m.mode(),
        m.len(),
        m.ctime(),
        m.ctime_nsec()
    )
}

pub(crate) fn checked_file(
    path: &Path,
    flags: OFlags,
    allowed: &impl Fn(&Path) -> bool,
) -> Result<File, String> {
    if !allowed(path) {
        return Err("Path excluded by Files settings.".into());
    }
    let resolved = fs::canonicalize(path).map_err(|e| e.to_string())?;
    if !allowed(&resolved) {
        return Err("Target excluded by Files settings.".into());
    }
    let flags = if flags.contains(OFlags::PATH) {
        flags
    } else {
        flags | OFlags::NONBLOCK
    };
    let fd = openat2(
        CWD,
        &resolved,
        flags | OFlags::CLOEXEC,
        Mode::empty(),
        ResolveFlags::NO_SYMLINKS,
    )
    .map_err(|e| e.to_string())?;
    let file = File::from(fd);
    if fd_path(&file)? != resolved {
        return Err("Path changed. Refresh and try again.".into());
    }
    Ok(file)
}

pub(crate) fn fd_path(file: &File) -> Result<PathBuf, String> {
    fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd())).map_err(|e| e.to_string())
}

pub fn read(path: &Path, params: &Value, allowed: impl Fn(&Path) -> bool) -> Result<Value, String> {
    if !allowed(path) {
        return Err("Path excluded by Files settings.".into());
    }
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    let initial_version = version(&meta);
    if params["version"]
        .as_str()
        .is_some_and(|v| v != initial_version)
    {
        return Err("File changed. Refresh and try again.".into());
    }
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let mime = if meta.is_dir() {
        "inode/directory".into()
    } else {
        gio::content_type_guess(Some(path), None).0.to_string()
    };
    let mut result = json!({"kind":"metadata","name":name,"path":path.to_string_lossy(),"hidden":name.starts_with('.'),"mime":mime,"size":meta.len(),"modified":meta.mtime(),"version":initial_version,"truncated":false});
    if params["preferences"]["files-preview"]["show_preview"] == false {
        return Ok(result);
    }
    if meta.is_dir() {
        let dir = checked_file(path, OFlags::DIRECTORY | OFlags::RDONLY, &allowed)?;
        let entries = fs::read_dir(format!("/proc/self/fd/{}", dir.as_raw_fd()))
            .map_err(|e| e.to_string())?;
        let show_hidden = params["preferences"]["files-search"]["show_hidden"]
            .as_bool()
            .unwrap_or(false);
        let mut children = Vec::new();
        let mut complete = true;
        let mut hidden_count = 0;
        for (index, entry) in entries.take(257).enumerate() {
            if index == 256 {
                complete = false;
                break;
            }
            let Ok(entry) = entry else {
                complete = false;
                continue;
            };
            let name = entry.file_name();
            if !allowed(&path.join(&name)) {
                continue;
            }
            if name.as_encoded_bytes().starts_with(b".") {
                hidden_count += 1;
                if !show_hidden {
                    continue;
                }
            }
            children.push((!entry.file_type().is_ok_and(|t| t.is_dir()), name));
        }
        if version(&dir.metadata().map_err(|e| e.to_string())?) != initial_version {
            return Err("Directory changed. Refresh and try again.".into());
        }
        children.sort_unstable();
        if complete {
            result["childCount"] = children.len().into();
            result["hiddenCount"] = hidden_count.into();
        }
        result["truncated"] = (!complete || children.len() > 8).into();
        result["children"] = children
            .into_iter()
            .take(8)
            .map(|(file, name)| json!({"name":name.to_string_lossy(),"directory":!file}))
            .collect::<Vec<_>>()
            .into();
        result["kind"] = "directory".into();
        return Ok(result);
    }
    if !meta.is_file() {
        return Ok(result);
    }
    let prefs = &params["preferences"]["files-preview"];
    let text_limit = prefs["text_max_bytes"]
        .as_u64()
        .unwrap_or(TEXT_LIMIT)
        .min(128 << 20);
    let image_limit = prefs["image_max_bytes"]
        .as_u64()
        .unwrap_or(IMAGE_LIMIT)
        .min(512 << 20);
    if meta.len() > text_limit.max(image_limit) {
        return Ok(result);
    }
    let handle = checked_file(path, OFlags::PATH, &allowed)?;
    if version(&handle.metadata().map_err(|e| e.to_string())?) != initial_version {
        return Err("File changed. Refresh and try again.".into());
    }
    let mut file =
        File::open(format!("/proc/self/fd/{}", handle.as_raw_fd())).map_err(|e| e.to_string())?;
    let mut prefix = Vec::new();
    (&mut file)
        .take(PREFIX as u64)
        .read_to_end(&mut prefix)
        .map_err(|e| e.to_string())?;
    let format = image::guess_format(&prefix).ok();
    if matches!(format, Some(ImageFormat::Jpeg | ImageFormat::Png)) && meta.len() <= image_limit {
        file.rewind().map_err(|e| e.to_string())?;
        let child = file.try_clone().map_err(|e| e.to_string())?;
        let out = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .arg("--preview-image")
            .stdin(Stdio::from(child))
            .stderr(Stdio::null())
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() && out.stdout.len() > 8 && out.stdout.len() < 320_000 {
            result["kind"] = "image".into();
            result["width"] = u32::from_be_bytes(out.stdout[..4].try_into().unwrap()).into();
            result["height"] = u32::from_be_bytes(out.stdout[4..8].try_into().unwrap()).into();
            result["imageBase64"] = STANDARD.encode(&out.stdout[8..]).into();
        } else {
            result["note"] = "Image preview unavailable within decoder limits.".into();
        }
    } else if meta.len() <= text_limit && !prefix.contains(&0) {
        let selected = params["line"].as_u64().filter(|n| *n > 0);
        if selected.is_some_and(|n| {
            prefix.iter().filter(|&&b| b == b'\n').count() < n.saturating_add(4) as usize
        }) {
            (&mut file)
                .take(256 * 1024)
                .read_to_end(&mut prefix)
                .map_err(|e| e.to_string())?;
        }
        let text = match std::str::from_utf8(&prefix) {
            Ok(text) => Some(text),
            Err(err) if err.error_len().is_none() && (prefix.len() as u64) < meta.len() => {
                std::str::from_utf8(&prefix[..err.valid_up_to()]).ok()
            }
            Err(_) => None,
        };
        if let Some(text) = text.filter(|s| {
            !s.chars()
                .any(|c| c.is_control() && !matches!(c, '\r' | '\n' | '\t'))
        }) {
            let start = selected.unwrap_or(1).saturating_sub(4) as usize;
            let begin = if start == 0 {
                Some(0)
            } else {
                text.match_indices('\n')
                    .nth(start - 1)
                    .map(|(at, _)| at + 1)
            };
            if let Some(begin) = begin {
                let tail = &text[begin..];
                let end = tail.floor_char_boundary(PREFIX.min(tail.len()));
                let lines: Vec<_> = tail[..end].lines().take(8).collect();
                result["truncated"] = (start > 0
                    || meta.len() > prefix.len() as u64
                    || end < tail.len()
                    || tail.lines().nth(8).is_some())
                .into();
                result["text"] = lines.join("\n").into();
                result["kind"] = "text".into();
                result["startLine"] = (start + 1).into();
                if let Some(selected) = selected {
                    result["selectedLine"] = selected.into();
                }
            } else {
                result["note"] = "Match is beyond the bounded text preview.".into();
            }
        }
    }
    if version(&file.metadata().map_err(|e| e.to_string())?) != initial_version
        || fs::symlink_metadata(path)
            .ok()
            .is_none_or(|m| version(&m) != initial_version)
    {
        return Err("File changed. Refresh and try again.".into());
    }
    Ok(result)
}

pub fn image_worker() -> ! {
    // A stalled decoder or read must not hold the Files process indefinitely.
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(4));
        std::process::exit(1)
    });
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        for (resource, limit) in [(Resource::Cpu, 2), (Resource::As, 256 << 20)] {
            setrlimit(
                resource,
                Rlimit {
                    current: Some(limit),
                    maximum: Some(limit),
                },
            )?;
        }
        let mut file = File::from(io::stdin().as_fd().try_clone_to_owned()?);
        let meta = file.metadata()?;
        if !meta.is_file() || meta.len() > 512 << 20 {
            return Err("Image too large".into());
        }
        let input = ImageReader::new(BufReader::new(&file)).with_guessed_format()?;
        if !matches!(input.format(), Some(ImageFormat::Png | ImageFormat::Jpeg)) {
            return Err("Unsupported format".into());
        }
        let (width, height) = input.into_dimensions()?;
        if u64::from(width) * u64::from(height) > 40_000_000 || width > 32768 || height > 32768 {
            return Err("Image too large".into());
        }
        file.rewind()?;
        let image = ImageReader::new(BufReader::new(file))
            .with_guessed_format()?
            .decode()?
            .thumbnail(256, 256);
        let mut png = Cursor::new(Vec::new());
        image.write_to(&mut png, ImageFormat::Png)?;
        let mut out = io::stdout().lock();
        out.write_all(&width.to_be_bytes())?;
        out.write_all(&height.to_be_bytes())?;
        out.write_all(png.get_ref())?;
        Ok(())
    })();
    std::process::exit(if result.is_ok() { 0 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_text_and_special_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("sample.txt");
        fs::write(&path, "line\n".repeat(3000)).unwrap();
        let preview = read(&path, &json!({"line":9}), |_| true).unwrap();
        assert_eq!(preview["truncated"], true);
        assert_eq!(preview["startLine"], 6);
        assert_eq!(preview["text"].as_str().unwrap().lines().count(), 8);
        fs::write(&path, b"x\0y").unwrap();
        assert_eq!(
            read(&path, &Value::Null, |_| true).unwrap()["kind"],
            "metadata"
        );
        std::os::unix::fs::symlink("/dev/zero", tmp.path().join("link")).unwrap();
        assert_eq!(
            read(&tmp.path().join("link"), &Value::Null, |_| true).unwrap()["kind"],
            "metadata"
        );
    }
}
