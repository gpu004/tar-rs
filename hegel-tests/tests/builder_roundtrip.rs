mod common;

use std::io::{Cursor, Read};

use common::{append_entries, draw_entries, InputEntry, Kind};
use hegel::TestCase;
use tar::{Archive, Builder, EntryType};

fn verify_reader(bytes: &[u8], expected: &[InputEntry], use_seek: bool) {
    let mut archive = Archive::new(Cursor::new(bytes));
    let entries = if use_seek {
        archive.entries_with_seek().unwrap()
    } else {
        archive.entries().unwrap()
    };
    let mut actual_count = 0;
    for (index, entry) in entries.enumerate() {
        let mut entry = entry.unwrap();
        let expected_entry = &expected[index];
        actual_count += 1;
        assert_eq!(entry.path_bytes().as_ref(), expected_entry.path.as_bytes());
        assert_eq!(
            entry.header().entry_type(),
            match &expected_entry.kind {
                Kind::Regular { .. } => EntryType::Regular,
                Kind::Dir => EntryType::Directory,
                Kind::Symlink { .. } => EntryType::Symlink,
                Kind::HardLink { .. } => EntryType::Link,
            }
        );
        let (size, contents) = match &expected_entry.kind {
            Kind::Regular { data, .. } => (data.len() as u64, data.as_slice()),
            _ => (0, &[][..]),
        };
        assert_eq!(entry.size(), size);
        if let Kind::Regular {
            mode,
            uid,
            gid,
            mtime,
            ..
        } = &expected_entry.kind
        {
            assert_eq!(entry.header().mode().unwrap(), *mode);
            assert_eq!(entry.header().uid().unwrap(), *uid);
            assert_eq!(entry.header().gid().unwrap(), *gid);
            assert_eq!(entry.header().mtime().unwrap(), *mtime);
        }
        let expected_link = match &expected_entry.kind {
            Kind::Symlink { target } | Kind::HardLink { target } => Some(target.as_bytes()),
            _ => None,
        };
        assert_eq!(
            entry.link_name_bytes().as_deref(),
            expected_link,
            "link name for {}",
            expected_entry.path
        );
        let mut data = Vec::new();
        entry.read_to_end(&mut data).unwrap();
        assert_eq!(data, contents, "contents for {}", expected_entry.path);
    }
    assert_eq!(actual_count, expected.len());
}

#[hegel::test(report_multiple_failures = true)]
fn builder_roundtrip(tc: TestCase) {
    let generated = draw_entries(&tc, 8, 3000);
    let mut builder = Builder::new(Vec::new());
    let appended = append_entries(&mut builder, &generated, &tc);
    builder.finish().unwrap();
    let bytes = builder.into_inner().unwrap();
    verify_reader(&bytes, &appended, false);
    verify_reader(&bytes, &appended, true);
}
