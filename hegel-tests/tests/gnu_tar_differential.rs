#![cfg(unix)]

mod common;

use std::collections::BTreeMap;
use std::fs::{self, symlink_metadata};
use std::io::{Cursor, Read};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::{append_entries, draw_segment, draw_tree_entries, ensure_files_readable, Kind};
use hegel::generators as gs;
use hegel::TestCase;
use tar::{Archive, Builder};
use tempfile::tempdir;

const GNU_TAR: &str = "/usr/bin/tar";

#[derive(Debug, PartialEq, Eq)]
enum FsEntry {
    Directory,
    File(Vec<u8>),
    Symlink(Vec<u8>),
}

fn archive_with_gnu_builder(tc: &TestCase) -> (Vec<u8>, Vec<(String, Kind)>) {
    let mut generated = draw_tree_entries(tc, 8, 3000);
    ensure_files_readable(&mut generated);
    let mut builder = Builder::new(Vec::new());
    let expected = append_entries(&mut builder, &generated, tc)
        .into_iter()
        .map(|entry| (entry.path, entry.kind))
        .collect();
    builder.finish().unwrap();
    (builder.into_inner().unwrap(), expected)
}

fn check_builder_archive_with_gnu_tar(tc: &TestCase) {
    let (bytes, expected) = archive_with_gnu_builder(tc);
    let temp = tempdir().unwrap();
    let archive_path = temp.path().join("input.tar");
    let extract_path = temp.path().join("extract");
    fs::create_dir(&extract_path).unwrap();
    fs::write(&archive_path, bytes).unwrap();

    let listing = Command::new(GNU_TAR)
        .arg("-tf")
        .arg(&archive_path)
        .arg("--quoting-style=literal")
        .output()
        .unwrap();
    assert!(
        listing.status.success(),
        "tar -tf failed: {}",
        String::from_utf8_lossy(&listing.stderr)
    );
    let mut listed: Vec<_> = String::from_utf8(listing.stdout)
        .unwrap()
        .lines()
        .map(|path| path.trim_end_matches('/').to_owned())
        .collect();
    let mut expected_names: Vec<_> = expected.iter().map(|(path, _)| path.clone()).collect();
    listed.sort();
    expected_names.sort();
    assert_eq!(listed, expected_names);

    let extraction = Command::new(GNU_TAR)
        .arg("-xf")
        .arg(&archive_path)
        .arg("-C")
        .arg(&extract_path)
        .output()
        .unwrap();
    assert!(
        extraction.status.success(),
        "tar -xf failed: {}",
        String::from_utf8_lossy(&extraction.stderr)
    );
    for (path, kind) in expected {
        let extracted = extract_path.join(&path);
        match kind {
            Kind::Regular { data, .. } => {
                assert!(symlink_metadata(&extracted).unwrap().file_type().is_file());
                assert_eq!(fs::read(extracted).unwrap(), data, "{path}");
            }
            Kind::Dir => {
                assert!(symlink_metadata(extracted).unwrap().file_type().is_dir());
            }
            Kind::Symlink { target } => {
                assert!(symlink_metadata(&extracted)
                    .unwrap()
                    .file_type()
                    .is_symlink());
                assert_eq!(
                    fs::read_link(extracted).unwrap(),
                    PathBuf::from(target),
                    "{path}"
                );
            }
            Kind::HardLink { .. } => unreachable!(),
        }
    }
}

fn normalized(path: &[u8]) -> String {
    let mut path = String::from_utf8(path.to_vec()).unwrap();
    while let Some(rest) = path.strip_prefix("./") {
        path = rest.to_owned();
    }
    path.trim_end_matches('/').to_owned()
}

fn filesystem_entries(root: &Path) -> BTreeMap<String, FsEntry> {
    fn visit(root: &Path, path: &Path, result: &mut BTreeMap<String, FsEntry>) {
        let metadata = symlink_metadata(path).unwrap();
        let relative = path.strip_prefix(root).unwrap();
        let name = relative.to_string_lossy().replace('\\', "/");
        if metadata.file_type().is_symlink() {
            let target = fs::read_link(path).unwrap();
            result.insert(
                name,
                FsEntry::Symlink(target.to_string_lossy().as_bytes().to_vec()),
            );
        } else if metadata.is_dir() {
            result.insert(name, FsEntry::Directory);
            for child in fs::read_dir(path).unwrap() {
                visit(root, &child.unwrap().path(), result);
            }
        } else if metadata.is_file() {
            result.insert(name, FsEntry::File(fs::read(path).unwrap()));
        }
    }

    let mut result = BTreeMap::new();
    for child in fs::read_dir(root).unwrap() {
        visit(root, &child.unwrap().path(), &mut result);
    }
    result
}

fn reverse_path(tc: &TestCase, format: &str, suffix: &str) -> String {
    match format {
        "gnu" | "pax" => format!("{}-{suffix}", draw_segment(tc, 240)),
        "ustar" => {
            let mut segments: Vec<_> = (0..4).map(|_| draw_segment(tc, 50)).collect();
            let last = segments.last_mut().unwrap();
            last.push_str(&format!("-{suffix}"));
            segments.join("/")
        }
        _ => format!("{}-{suffix}", draw_segment(tc, 76)),
    }
}

fn compare_gnu_tar_archive(tc: &TestCase, format: &str) {
    let temp = tempdir().unwrap();
    let tree = temp.path().join("tree");
    fs::create_dir(&tree).unwrap();
    let directory = reverse_path(tc, format, "dir");
    let file_path = format!("{directory}/file");
    let link_path = format!("{directory}/link");
    let target = draw_segment(tc, 40);
    let contents = tc.draw(gs::binary().max_size(300));
    fs::create_dir_all(tree.join(&directory)).unwrap();
    fs::write(tree.join(&file_path), &contents).unwrap();
    symlink(&target, tree.join(&link_path)).unwrap();
    let archive_path = temp.path().join("reverse.tar");
    let output = Command::new(GNU_TAR)
        .arg(format!("--format={format}"))
        .arg("-cf")
        .arg(&archive_path)
        .arg("-C")
        .arg(&tree)
        .arg(".")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "GNU tar format {format} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let expected = filesystem_entries(&tree);
    let mut archive = Archive::new(Cursor::new(fs::read(archive_path).unwrap()));
    let mut actual = BTreeMap::new();
    for entry in archive.entries().unwrap() {
        let mut entry = entry.unwrap();
        let path = normalized(&entry.path_bytes());
        if path.is_empty() {
            continue;
        }
        let kind = entry.header().entry_type();
        let item = if kind.is_dir() {
            FsEntry::Directory
        } else if kind.is_symlink() {
            FsEntry::Symlink(entry.link_name_bytes().unwrap().as_ref().to_vec())
        } else {
            let mut contents = Vec::new();
            entry.read_to_end(&mut contents).unwrap();
            FsEntry::File(contents)
        };
        assert!(
            actual.insert(path.clone(), item).is_none(),
            "duplicate {path}"
        );
    }
    assert_eq!(actual, expected, "GNU tar {format}");
}

#[hegel::test(test_cases = 50, report_multiple_failures = true)]
fn gnu_tar_reads_builder_archives(tc: TestCase) {
    check_builder_archive_with_gnu_tar(&tc);
}

#[hegel::test(test_cases = 50, report_multiple_failures = true)]
fn tar_reads_gnu_archives(tc: TestCase) {
    for format in ["gnu", "pax", "ustar", "v7"] {
        compare_gnu_tar_archive(&tc, format);
    }
}
