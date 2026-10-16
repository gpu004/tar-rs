mod common;

use std::io::Cursor;
use std::time::{Duration, Instant};

use common::{build_archive, draw_entries, read_entries, ObservedEntry};
use hegel::generators as gs;
use hegel::TestCase;
use tar::Header;

const PARSE_TIME_LIMIT: Duration = Duration::from_secs(2);

fn inspect(bytes: &[u8]) {
    let started = Instant::now();
    let normal = read_entries(Cursor::new(bytes.to_vec()), false);
    assert!(
        started.elapsed() < PARSE_TIME_LIMIT,
        "entries() exceeded time budget"
    );
    let with_seek = read_entries(Cursor::new(bytes.to_vec()), true);
    assert!(
        started.elapsed() < PARSE_TIME_LIMIT,
        "entries_with_seek() exceeded time budget"
    );
    assert_reads_match_declared_size(&normal);
    assert_reads_match_declared_size(&with_seek);
    assert_eq!(normal, with_seek);
}

fn inspect_safety(bytes: &[u8]) {
    let started = Instant::now();
    drop(read_entries(Cursor::new(bytes.to_vec()), false));
    assert!(
        started.elapsed() < PARSE_TIME_LIMIT,
        "entries() exceeded time budget"
    );
    drop(read_entries(Cursor::new(bytes.to_vec()), true));
    assert!(
        started.elapsed() < PARSE_TIME_LIMIT,
        "entries_with_seek() exceeded time budget"
    );
}

fn assert_reads_match_declared_size(entries: &[Result<ObservedEntry, std::io::ErrorKind>]) {
    for entry in entries.iter().flatten() {
        if let Ok(contents) = &entry.contents {
            assert_eq!(
                contents.len() as u64,
                entry.size,
                "entry read size differs from declared size"
            );
        }
    }
}

fn mutated_archive(tc: &TestCase) -> Vec<u8> {
    let entries = draw_entries(tc, 6, 160);
    let mut bytes = build_archive(&entries, tc);
    let mutation_count = tc.draw(gs::integers::<usize>().min_value(1).max_value(8));
    for _ in 0..mutation_count {
        let operation = tc.draw(gs::integers::<u8>().max_value(2));
        match operation {
            0 if !bytes.is_empty() => {
                let index = tc.draw(gs::integers::<usize>().max_value(bytes.len() - 1));
                bytes[index] = tc.draw(gs::integers::<u8>());
            }
            1 => {
                let index = tc.draw(gs::integers::<usize>().max_value(bytes.len()));
                let splice = tc.draw(gs::binary().max_size(24));
                bytes.splice(index..index, splice);
            }
            _ if !bytes.is_empty() => {
                let new_len = tc.draw(gs::integers::<usize>().max_value(bytes.len()));
                bytes.truncate(new_len);
            }
            _ => bytes.push(tc.draw(gs::integers::<u8>())),
        }
    }
    if tc.draw(gs::booleans()) {
        let complete_blocks = bytes.len() / 512;
        for block in bytes.chunks_mut(512).take(complete_blocks) {
            let mut header = Header::from_byte_slice(block).clone();
            header.set_cksum();
            block.copy_from_slice(header.as_bytes());
        }
    }
    bytes
}

#[hegel::test(report_multiple_failures = true)]
fn parse_arbitrary_bytes(tc: TestCase) {
    let bytes = tc.draw(gs::binary().max_size(4096));
    inspect(&bytes);
}

#[hegel::test(report_multiple_failures = true)]
fn parse_mutated_valid_archives_terminate(tc: TestCase) {
    let bytes = mutated_archive(&tc);
    inspect_safety(&bytes);
}

#[hegel::test(report_multiple_failures = true)]
#[hegel::reproduce_failure(
    "AXicbY1RCgAgCEP3rK8u0DW6/+lSpEhIYXOi25TXwMGS6IErJu/cqemeVaKqzy/V/Ria9CaHxGADY1QBOQ=="
)]
#[ignore = "fails: entries() reports a trailing error that entries_with_seek() omits (https://github.com/gpu004/tar-rs/issues/2)"]
fn parse_mutated_valid_archives_reader_equivalence(tc: TestCase) {
    let bytes = mutated_archive(&tc);
    let started = Instant::now();
    let normal = read_entries(Cursor::new(bytes.clone()), false);
    assert!(
        started.elapsed() < PARSE_TIME_LIMIT,
        "entries() exceeded time budget"
    );
    let with_seek = read_entries(Cursor::new(bytes), true);
    assert!(
        started.elapsed() < PARSE_TIME_LIMIT,
        "entries_with_seek() exceeded time budget"
    );
    assert_eq!(normal, with_seek);
}

#[hegel::test(report_multiple_failures = true)]
#[hegel::reproduce_failure(
    "AXicbU1BDgAgCALr1Af6Rv9/XZqrxaZuIE5hwmvQwZLYA1dM3rlDwztToqril+p+DQ34k480cgNjUAE4"
)]
#[ignore = "fails: truncated entry reads fewer bytes than its declared size (https://github.com/gpu004/tar-rs/issues/1)"]
fn parse_mutated_valid_archives_declared_size(tc: TestCase) {
    let bytes = mutated_archive(&tc);
    let started = Instant::now();
    let normal = read_entries(Cursor::new(bytes.clone()), false);
    assert_reads_match_declared_size(&normal);
    assert!(
        started.elapsed() < PARSE_TIME_LIMIT,
        "entries() exceeded time budget"
    );
    let with_seek = read_entries(Cursor::new(bytes), true);
    assert_reads_match_declared_size(&with_seek);
    assert!(
        started.elapsed() < PARSE_TIME_LIMIT,
        "entries_with_seek() exceeded time budget"
    );
}
