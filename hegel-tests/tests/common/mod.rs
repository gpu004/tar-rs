#![allow(dead_code)]

use std::io::{self, Cursor, Read, Seek};
use std::time::{Duration, Instant};

use hegel::generators as gs;
use hegel::TestCase;
use tar::{Builder, EntryType, Header};

#[derive(Clone, Debug)]
pub enum Kind {
    Regular {
        data: Vec<u8>,
        mode: u32,
        uid: u64,
        gid: u64,
        mtime: u64,
    },
    Dir,
    Symlink {
        target: String,
    },
    HardLink {
        target: String,
    },
}

#[derive(Clone, Debug)]
pub struct InputEntry {
    pub path: String,
    pub kind: Kind,
    pub gnu: bool,
}

pub fn draw_segment(tc: &TestCase, max_size: usize) -> String {
    let segment = tc.draw(
        gs::text()
            .alphabet("abcdefghijklmnopqrstuvwxyz0123456789._-")
            .min_size(1)
            .max_size(max_size),
    );
    if segment == "." || segment == ".." {
        format!("{segment}_")
    } else {
        segment
    }
}

pub fn draw_path(tc: &TestCase, max_segments: usize, max_segment_size: usize) -> String {
    let count = tc.draw(gs::integers::<usize>().min_value(1).max_value(max_segments));
    let mut parts = Vec::with_capacity(count);
    for _ in 0..count {
        parts.push(draw_segment(tc, max_segment_size));
    }
    let trailing_slash = tc.draw(gs::booleans());
    let mut path = parts.join("/");
    if trailing_slash {
        path.push('/');
    }
    path
}

pub fn draw_entries(tc: &TestCase, max_entries: usize, max_data: usize) -> Vec<InputEntry> {
    draw_entries_with_hardlinks(tc, max_entries, max_data, true)
}

pub fn draw_tree_entries(tc: &TestCase, max_entries: usize, max_data: usize) -> Vec<InputEntry> {
    draw_entries_with_hardlinks(tc, max_entries, max_data, false)
}

pub fn ensure_files_readable(entries: &mut [InputEntry]) {
    for entry in entries {
        if let Kind::Regular { mode, .. } = &mut entry.kind {
            *mode |= 0o400;
        }
    }
}

fn draw_entries_with_hardlinks(
    tc: &TestCase,
    max_entries: usize,
    max_data: usize,
    hardlinks: bool,
) -> Vec<InputEntry> {
    let count = tc.draw(gs::integers::<usize>().min_value(0).max_value(max_entries));
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let mut path = format!("entry-{index}/{}", draw_path(tc, 4, 70));
        path.push_str(&format!("-entry-{index}"));
        let gnu = tc.draw(gs::booleans());
        let maximum_kind = if hardlinks { 3 } else { 2 };
        let kind = match tc.draw(gs::integers::<u8>().max_value(maximum_kind)) {
            0 => Kind::Regular {
                data: tc.draw(gs::binary().max_size(max_data)),
                mode: tc.draw(gs::integers::<u32>().max_value(0o7777)),
                uid: tc.draw(gs::integers::<u64>().max_value(1_000_000)),
                gid: tc.draw(gs::integers::<u64>().max_value(1_000_000)),
                mtime: tc.draw(gs::integers::<u64>().max_value(2_000_000_000)),
            },
            1 => Kind::Dir,
            2 => Kind::Symlink {
                target: draw_path(tc, 4, 70),
            },
            _ => Kind::HardLink {
                target: draw_path(tc, 4, 70),
            },
        };
        entries.push(InputEntry { path, kind, gnu });
    }
    entries
}

pub fn path_fits_ustar(path: &str) -> bool {
    if path.len() <= 100 {
        return true;
    }
    if path.len() > 255 {
        return false;
    }
    path.match_indices('/')
        .any(|(index, _)| index <= 155 && path.len() - index - 1 <= 100)
}

pub fn append_entries(
    builder: &mut Builder<Vec<u8>>,
    entries: &[InputEntry],
    tc: &TestCase,
) -> Vec<InputEntry> {
    let mut appended = Vec::new();
    for entry in entries {
        let mut header = if entry.gnu {
            Header::new_gnu()
        } else {
            Header::new_ustar()
        };
        let entry_type = match &entry.kind {
            Kind::Regular { .. } => EntryType::Regular,
            Kind::Dir => EntryType::Directory,
            Kind::Symlink { .. } => EntryType::Symlink,
            Kind::HardLink { .. } => EntryType::Link,
        };
        header.set_entry_type(entry_type);
        header.set_size(match &entry.kind {
            Kind::Regular { data, .. } => data.len() as u64,
            _ => 0,
        });
        if let Kind::Regular {
            mode,
            uid,
            gid,
            mtime,
            ..
        } = &entry.kind
        {
            header.set_mode(*mode);
            header.set_uid(*uid);
            header.set_gid(*gid);
            header.set_mtime(*mtime);
        }
        let result = match &entry.kind {
            Kind::Regular { data, .. } => {
                builder.append_data(&mut header, &entry.path, Cursor::new(data))
            }
            Kind::Dir => builder.append_data(&mut header, &entry.path, io::empty()),
            Kind::Symlink { target } | Kind::HardLink { target } => {
                builder.append_link(&mut header, &entry.path, target)
            }
        };
        if let Err(error) = result {
            let documented_ustar_limit = !entry.gnu
                && (!path_fits_ustar(&entry.path)
                    || matches!(&entry.kind, Kind::Symlink { target } | Kind::HardLink { target } if target.len() > 100));
            if documented_ustar_limit {
                tc.note(&format!(
                    "skipped documented UStar path/link limit: {:?} -> {:?}: {error}",
                    entry.path, entry.kind
                ));
            } else {
                panic!(
                    "builder rejected generated input {:?} -> {:?}: {error}",
                    entry.path, entry.kind
                );
            }
        } else {
            appended.push(entry.clone());
        }
    }
    appended
}

pub fn build_archive(entries: &[InputEntry], tc: &TestCase) -> Vec<u8> {
    let mut builder = Builder::new(Vec::new());
    append_entries(&mut builder, entries, tc);
    builder.finish().unwrap();
    builder.into_inner().unwrap()
}

#[derive(Debug, PartialEq, Eq)]
pub struct ObservedEntry {
    pub path: Vec<u8>,
    pub size: u64,
    pub contents: Result<Vec<u8>, io::ErrorKind>,
}

pub fn read_entries<R: Read + Seek + 'static>(
    reader: R,
    seek: bool,
) -> Vec<Result<ObservedEntry, io::ErrorKind>> {
    let started = Instant::now();
    let mut archive = tar::Archive::new(reader);
    let entries = if seek {
        archive.entries_with_seek()
    } else {
        archive.entries()
    };
    let entries = match entries {
        Ok(entries) => entries,
        Err(error) => return vec![Err(error.kind())],
    };
    let mut observed = Vec::new();
    for (count, entry) in entries.enumerate() {
        assert!(count < 10_000, "archive yielded more than 10,000 entries");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "archive parsing exceeded time budget"
        );
        match entry {
            Err(error) => observed.push(Err(error.kind())),
            Ok(mut entry) => {
                let path = entry.path_bytes().into_owned();
                let _link_name = entry.link_name_bytes();
                let header = entry.header();
                let _size = header.size();
                let _uid = header.uid();
                let _gid = header.gid();
                let _mode = header.mode();
                let _mtime = header.mtime();
                let _device_major = header.device_major();
                let _device_minor = header.device_minor();
                if let Some(gnu) = header.as_gnu() {
                    let _atime = gnu.atime();
                    let _ctime = gnu.ctime();
                    let _real_size = gnu.real_size();
                }
                if let Ok(Some(extensions)) = entry.pax_extensions() {
                    for _extension in extensions {}
                }
                let size = entry.size();
                let mut contents = Vec::new();
                let contents = match entry.read_to_end(&mut contents) {
                    Ok(_) => Ok(contents),
                    Err(error) => Err(error.kind()),
                };
                observed.push(Ok(ObservedEntry {
                    path,
                    size,
                    contents,
                }));
            }
        }
    }
    observed
}
