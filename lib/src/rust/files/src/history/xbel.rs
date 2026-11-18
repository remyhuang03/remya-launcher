//! GLib 0.21 exposes GBookmarkFile through its FFI only.
use gio::glib::{GString, ffi, translate::FromGlibContainer};
use std::{
    ffi::{CStr, CString},
    fs::{File, OpenOptions},
    io::{ErrorKind, Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
    ptr::{self, NonNull},
};
use url::Url;

const MAX_BYTES: u64 = 16 * 1024 * 1024;
const APP: &CStr = c"top.remya.launcher";

pub(super) struct Book(NonNull<ffi::GBookmarkFile>);

impl Drop for Book {
    fn drop(&mut self) {
        unsafe { ffi::g_bookmark_file_free(self.0.as_ptr()) };
    }
}

impl Book {
    pub fn load(path: &Path) -> Result<Self, String> {
        let book = Self(NonNull::new(unsafe { ffi::g_bookmark_file_new() }).unwrap());
        let mut options = OpenOptions::new();
        options.read(true).custom_flags(
            rustix::fs::OFlags::NONBLOCK.bits() as i32 | rustix::fs::OFlags::NOFOLLOW.bits() as i32,
        );
        let file = match options.open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(book),
            Err(_) => return Err("HistoryUnavailable".into()),
        };
        if !file
            .metadata()
            .is_ok_and(|m| m.is_file() && m.len() <= MAX_BYTES)
        {
            return Err("HistoryUnavailable".into());
        }
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "HistoryUnavailable")?;
        if bytes.is_empty()
            || bytes.len() as u64 > MAX_BYTES
            || unsafe {
                ffi::g_bookmark_file_load_from_data(
                    book.0.as_ptr(),
                    bytes.as_ptr(),
                    bytes.len(),
                    ptr::null_mut(),
                )
            } == 0
        {
            return Err("HistoryInvalid".into());
        }
        Ok(book)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let mut len = 0;
        let data =
            unsafe { ffi::g_bookmark_file_to_data(self.0.as_ptr(), &mut len, ptr::null_mut()) };
        if data.is_null() {
            return Err("HistoryWriteFailed".into());
        }
        let bytes = unsafe { std::slice::from_raw_parts(data, len) };
        let dir = path.parent().ok_or("HistoryUnavailable")?;
        let saved = (|| -> std::io::Result<()> {
            let mut temp = tempfile::NamedTempFile::new_in(dir)?;
            temp.write_all(bytes)?;
            temp.as_file().sync_all()?;
            temp.persist(path)?;
            File::open(dir)?.sync_all()
        })();
        unsafe { ffi::g_free(data.cast()) };
        saved.map_err(|_| "HistoryWriteFailed".into())
    }

    fn uris(&self) -> Vec<GString> {
        let mut len = 0;
        unsafe {
            let items = ffi::g_bookmark_file_get_uris(self.0.as_ptr(), &mut len);
            Vec::<GString>::from_glib_full_num(items, len)
        }
    }

    fn count(&self, uri: &CStr) -> u32 {
        let mut count = 0;
        unsafe {
            ffi::g_bookmark_file_get_app_info(
                self.0.as_ptr(),
                uri.as_ptr(),
                APP.as_ptr(),
                ptr::null_mut(),
                &mut count,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        count
    }

    fn visited(&self, uri: &CStr) -> i64 {
        unsafe { ffi::g_bookmark_file_get_visited(self.0.as_ptr(), uri.as_ptr(), ptr::null_mut()) }
    }

    pub fn entries(&self, shared: bool) -> Vec<(PathBuf, i64, u32)> {
        self.uris()
            .into_iter()
            .filter_map(|s| {
                let url = Url::parse(s.as_str()).ok()?;
                if url.scheme() != "file"
                    || url.host_str().is_some_and(|s| s != "localhost")
                    || url.query().is_some()
                    || url.fragment().is_some()
                {
                    return None;
                }
                let path = url.to_file_path().ok()?;
                let uri = s.as_gstr().to_cstr_until_nul();
                if shared
                    && unsafe {
                        ffi::g_bookmark_file_get_is_private(
                            self.0.as_ptr(),
                            uri.as_ptr(),
                            ptr::null_mut(),
                        )
                    } != 0
                {
                    return None;
                }
                Some((
                    path,
                    self.visited(uri),
                    if shared { 0 } else { self.count(uri) },
                ))
            })
            .collect()
    }

    pub fn record(
        &mut self,
        path: &Path,
        now: i64,
        cutoff: i64,
        limit: usize,
    ) -> Result<(), String> {
        let uri = uri(path)?;
        let count = self.count(&uri).saturating_add(1).min(i32::MAX as u32);
        if unsafe {
            ffi::g_bookmark_file_set_app_info(
                self.0.as_ptr(),
                uri.as_ptr(),
                APP.as_ptr(),
                c"remya_launcher %u".as_ptr(),
                count as i32,
                now,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err("HistoryWriteFailed".into());
        }
        unsafe {
            ffi::g_bookmark_file_set_visited(self.0.as_ptr(), uri.as_ptr(), now);
        }
        let mut entries: Vec<_> = self
            .uris()
            .into_iter()
            .map(|s| (self.visited(s.as_gstr().to_cstr_until_nul()), s))
            .collect();
        entries.sort_by(|a, b| {
            b.0.cmp(&a.0).then_with(|| {
                (b.1.as_gstr().to_cstr_until_nul() == uri.as_c_str())
                    .cmp(&(a.1.as_gstr().to_cstr_until_nul() == uri.as_c_str()))
            })
        });
        for (index, (visited, s)) in entries.into_iter().enumerate() {
            if visited < cutoff || index >= limit {
                unsafe {
                    ffi::g_bookmark_file_remove_item(
                        self.0.as_ptr(),
                        s.as_gstr().as_ptr(),
                        ptr::null_mut(),
                    );
                }
            }
        }
        Ok(())
    }

    pub fn move_item(&mut self, from: &Path, to: Option<&Path>) -> Result<bool, String> {
        let from = uri(from)?;
        let to = to.map(uri).transpose()?;
        let book = self.0.as_ptr();
        if unsafe { ffi::g_bookmark_file_has_item(book, from.as_ptr()) } == 0 {
            return Ok(false);
        }
        let ok = unsafe {
            ffi::g_bookmark_file_move_item(
                book,
                from.as_ptr(),
                to.as_ref().map_or(ptr::null(), |s| s.as_ptr()),
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err("HistoryWriteFailed".into());
        }
        Ok(true)
    }
}

fn uri(path: &Path) -> Result<CString, String> {
    let url = Url::from_file_path(path).map_err(|_| "InvalidPath")?;
    CString::new(url.as_str()).map_err(|_| "InvalidPath".into())
}
