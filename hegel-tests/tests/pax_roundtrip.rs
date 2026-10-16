use std::io::Cursor;

use hegel::generators as gs;
use hegel::TestCase;
use tar::{Archive, Builder, EntryType, Header};

fn draw_extensions(tc: &TestCase, equals_allowed: bool) -> Vec<(String, Vec<u8>)> {
    let count = tc.draw(gs::integers::<usize>().min_value(1).max_value(5));
    let alphabet = if equals_allowed {
        "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 =\n"
    } else {
        "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 _-\n"
    };
    (0..count)
        .map(|_| {
            let key = tc.draw(gs::text().alphabet(alphabet).min_size(1).max_size(16));
            let value = tc.draw(gs::binary().max_size(80));
            (key, value)
        })
        .collect()
}

fn assert_pax_roundtrip(tc: &TestCase, extensions: Vec<(String, Vec<u8>)>) {
    let payload = tc.draw(gs::binary().max_size(20));
    let mut builder = Builder::new(Vec::new());
    builder
        .append_pax_extensions(
            extensions
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_slice())),
        )
        .unwrap();
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Regular);
    header.set_size(payload.len() as u64);
    builder
        .append_data(&mut header, "payload", Cursor::new(&payload))
        .unwrap();
    builder.finish().unwrap();
    let bytes = builder.into_inner().unwrap();

    let mut archive = Archive::new(Cursor::new(bytes));
    let mut entries = archive.entries().unwrap();
    let mut entry = entries.next().unwrap().unwrap();
    let pax = entry.pax_extensions().unwrap().unwrap();
    let actual: Vec<_> = pax
        .map(|extension| {
            let extension = extension.unwrap();
            (
                extension.key_bytes().to_vec(),
                extension.value_bytes().to_vec(),
            )
        })
        .collect();
    let expected: Vec<_> = extensions
        .iter()
        .map(|(key, value)| (key.as_bytes().to_vec(), value.clone()))
        .collect();
    assert_eq!(actual, expected);
}

#[hegel::test(report_multiple_failures = true)]
fn pax_roundtrip_keys_without_equals(tc: TestCase) {
    let extensions = draw_extensions(&tc, false);
    assert_pax_roundtrip(&tc, extensions);
}

#[hegel::test(report_multiple_failures = true)]
#[hegel::reproduce_failure("AXicY2EAAi5GIMHIwgQkDYDYFoiZGWAEAAlrAIo=")]
#[ignore = "fails: PAX key ending in '=' is parsed as part of its value (https://github.com/gpu004/tar-rs/issues/3)"]
fn pax_roundtrip_key_ending_in_equals(tc: TestCase) {
    let extensions = draw_extensions(&tc, true);
    assert_pax_roundtrip(&tc, extensions);
}

#[hegel::test(report_multiple_failures = true)]
#[hegel::reproduce_failure("AXicY2EAAi5GIMHIAiJtgZiZAUYAAAWdAFk=")]
#[ignore = "fails: PAX key '=' is rejected as malformed (https://github.com/gpu004/tar-rs/issues/3)"]
fn pax_roundtrip_equals_only_key(tc: TestCase) {
    let extensions = draw_extensions(&tc, true);
    assert_pax_roundtrip(&tc, extensions);
}

#[hegel::test(report_multiple_failures = true)]
fn pax_metadata_overrides(tc: TestCase) {
    let path = tc.draw(
        gs::text()
            .alphabet("abcdefghijklmnopqrstuvwxyz0123456789._-")
            .min_size(1)
            .max_size(60),
    );
    let linkpath = tc.draw(
        gs::text()
            .alphabet("abcdefghijklmnopqrstuvwxyz0123456789._-")
            .min_size(1)
            .max_size(60),
    );
    let payload = tc.draw(gs::binary().max_size(80));
    let uid = tc.draw(gs::integers::<u64>().max_value(1_000_000));
    let gid = tc.draw(gs::integers::<u64>().max_value(1_000_000));
    let values = [
        ("path", path.as_bytes().to_vec()),
        ("linkpath", linkpath.as_bytes().to_vec()),
        ("size", payload.len().to_string().into_bytes()),
        ("uid", uid.to_string().into_bytes()),
        ("gid", gid.to_string().into_bytes()),
    ];

    let mut builder = Builder::new(Vec::new());
    builder
        .append_pax_extensions(values.iter().map(|(key, value)| (*key, value.as_slice())))
        .unwrap();
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Regular);
    header.set_size(payload.len() as u64);
    builder
        .append_data(&mut header, "placeholder", Cursor::new(&payload))
        .unwrap();
    builder.finish().unwrap();

    let mut archive = Archive::new(Cursor::new(builder.into_inner().unwrap()));
    let mut entries = archive.entries().unwrap();
    let entry = entries.next().unwrap().unwrap();
    assert_eq!(entry.path_bytes().as_ref(), path.as_bytes());
    assert_eq!(
        entry.link_name_bytes().as_deref(),
        Some(linkpath.as_bytes())
    );
    assert_eq!(entry.size(), payload.len() as u64);
    assert_eq!(entry.header().uid().unwrap(), uid);
    assert_eq!(entry.header().gid().unwrap(), gid);
}
