#![cfg(unix)]

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::io::Cursor;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use common::{build_archive, draw_entries, ensure_files_readable};
use hegel::generators as gs;
use hegel::TestCase;
use tar::{Archive, Builder, EntryType, Header};
use tempfile::tempdir;

#[derive(Debug, PartialEq, Eq)]
struct Snapshot {
    kind: &'static str,
    contents: Vec<u8>,
    link: Option<PathBuf>,
    mode: u32,
    mtime: i64,
    mtime_nsec: i64,
    inode: u64,
}

fn snapshot(root: &Path) -> BTreeMap<PathBuf, Snapshot> {
    fn visit(root: &Path, path: &Path, result: &mut BTreeMap<PathBuf, Snapshot>) {
        let metadata = fs::symlink_metadata(path).unwrap();
        let file_type = metadata.file_type();
        let (kind, contents, link) = if file_type.is_symlink() {
            ("symlink", Vec::new(), Some(fs::read_link(path).unwrap()))
        } else if file_type.is_dir() {
            ("directory", Vec::new(), None)
        } else if file_type.is_file() {
            ("file", fs::read(path).unwrap(), None)
        } else {
            ("other", Vec::new(), None)
        };
        result.insert(
            path.strip_prefix(root).unwrap().to_path_buf(),
            Snapshot {
                kind,
                contents,
                link,
                mode: metadata.mode(),
                mtime: metadata.mtime(),
                mtime_nsec: metadata.mtime_nsec(),
                inode: metadata.ino(),
            },
        );
        if file_type.is_dir() {
            for child in fs::read_dir(path).unwrap() {
                visit(root, &child.unwrap().path(), result);
            }
        }
    }

    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}

fn assert_no_changes_outside_dst(root: &Path, dst: &Path, before: BTreeMap<PathBuf, Snapshot>) {
    let dst_relative = dst.strip_prefix(root).unwrap();
    let after = snapshot(root);
    let outside_before: BTreeMap<_, _> = before
        .into_iter()
        .filter(|(path, _)| !path.starts_with(dst_relative))
        .collect();
    let outside_after: BTreeMap<_, _> = after
        .into_iter()
        .filter(|(path, _)| !path.starts_with(dst_relative))
        .collect();
    assert_eq!(outside_after, outside_before);
}

fn unpack_and_check(bytes: Vec<u8>) {
    let root = tempdir().unwrap();
    let outer = root.path().join("outer");
    let dst = outer.join("dst");
    let sibling = root.path().join("sibling");
    fs::create_dir_all(&dst).unwrap();
    fs::create_dir_all(&sibling).unwrap();
    fs::write(outer.join("sentinel"), b"outer sentinel").unwrap();
    fs::write(sibling.join("sentinel"), b"sibling sentinel").unwrap();
    let before = snapshot(root.path());

    let mut archive = Archive::new(Cursor::new(bytes));
    let _result = archive.unpack(&dst);

    assert_no_changes_outside_dst(root.path(), &dst, before);
    assert_eq!(fs::read(outer.join("sentinel")).unwrap(), b"outer sentinel");
    assert_eq!(
        fs::read(sibling.join("sentinel")).unwrap(),
        b"sibling sentinel"
    );
}

fn raw_entry(builder: &mut Builder<Vec<u8>>, path: &[u8], ty: EntryType, link: &[u8], data: &[u8]) {
    assert!(path.len() <= 100);
    assert!(link.len() <= 100);
    let mut header = Header::new_gnu();
    header.as_old_mut().name.fill(0);
    header.as_old_mut().name[..path.len()].copy_from_slice(path);
    header.as_old_mut().linkname.fill(0);
    header.as_old_mut().linkname[..link.len()].copy_from_slice(link);
    header.set_entry_type(ty);
    header.set_size(data.len() as u64);
    header.set_cksum();
    builder.append(&header, Cursor::new(data)).unwrap();
}

fn adversarial_archive(sibling: &Path) -> Vec<u8> {
    let absolute = sibling.join("sentinel").to_string_lossy().into_owned();
    let mut builder = Builder::new(Vec::new());
    raw_entry(
        &mut builder,
        b"../sentinel",
        EntryType::Regular,
        b"",
        b"escape",
    );
    raw_entry(
        &mut builder,
        absolute.as_bytes(),
        EntryType::Regular,
        b"",
        b"absolute",
    );
    raw_entry(
        &mut builder,
        b"link",
        EntryType::Symlink,
        b"../../sibling",
        b"",
    );
    raw_entry(
        &mut builder,
        b"link/sentinel",
        EntryType::Regular,
        b"",
        b"through symlink",
    );
    raw_entry(
        &mut builder,
        b"hard",
        EntryType::Link,
        b"../../sibling/sentinel",
        b"",
    );
    builder.finish().unwrap();
    builder.into_inner().unwrap()
}

fn mutated_archive(tc: &TestCase) -> Vec<u8> {
    let mut entries = draw_entries(tc, 8, 128);
    ensure_files_readable(&mut entries);
    let mut bytes = build_archive(&entries, tc);
    let mutation_count = tc.draw(gs::integers::<usize>().min_value(1).max_value(8));
    for _ in 0..mutation_count {
        match tc.draw(gs::integers::<u8>().max_value(2)) {
            0 if !bytes.is_empty() => {
                let index = tc.draw(gs::integers::<usize>().max_value(bytes.len() - 1));
                bytes[index] = tc.draw(gs::integers::<u8>());
            }
            1 => {
                let index = tc.draw(gs::integers::<usize>().max_value(bytes.len()));
                bytes.splice(index..index, tc.draw(gs::binary().max_size(24)));
            }
            _ if !bytes.is_empty() => {
                let length = tc.draw(gs::integers::<usize>().max_value(bytes.len()));
                bytes.truncate(length);
            }
            _ => bytes.push(tc.draw(gs::integers::<u8>())),
        }
    }
    bytes
}

#[hegel::test(report_multiple_failures = true)]
fn unpack_containment_generated_archives(tc: TestCase) {
    let mut entries = draw_entries(&tc, 8, 128);
    ensure_files_readable(&mut entries);
    unpack_and_check(build_archive(&entries, &tc));
}

#[hegel::test(report_multiple_failures = true)]
fn unpack_containment_mutated_archives(tc: TestCase) {
    unpack_and_check(mutated_archive(&tc));
}

#[hegel::test(report_multiple_failures = true)]
fn unpack_containment_adversarial_paths(_tc: TestCase) {
    let root = tempdir().unwrap();
    let outer = root.path().join("outer");
    let sibling = root.path().join("sibling");
    fs::create_dir_all(outer.join("dst")).unwrap();
    fs::create_dir_all(&sibling).unwrap();
    fs::write(outer.join("sentinel"), b"outer sentinel").unwrap();
    fs::write(sibling.join("sentinel"), b"sibling sentinel").unwrap();
    let before = snapshot(root.path());
    let bytes = adversarial_archive(&sibling);
    let mut archive = Archive::new(Cursor::new(bytes));
    let _result = archive.unpack(outer.join("dst"));
    assert_no_changes_outside_dst(root.path(), &outer.join("dst"), before);
    assert_eq!(fs::read(outer.join("sentinel")).unwrap(), b"outer sentinel");
    assert_eq!(
        fs::read(sibling.join("sentinel")).unwrap(),
        b"sibling sentinel"
    );
}
