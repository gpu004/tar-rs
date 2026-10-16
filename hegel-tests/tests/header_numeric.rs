use hegel::generators as gs;
use hegel::TestCase;
use tar::Header;

type NumericDraws = (u64, u64, u64, u64, u32, u32, u32, u64, u64, u64);

fn draw_full_range(tc: &TestCase) -> NumericDraws {
    (
        tc.draw(gs::integers::<u64>()),
        tc.draw(gs::integers::<u64>()),
        tc.draw(gs::integers::<u64>()),
        tc.draw(gs::integers::<u64>()),
        tc.draw(gs::integers::<u32>()),
        tc.draw(gs::integers::<u32>()),
        tc.draw(gs::integers::<u32>()),
        tc.draw(gs::integers::<u64>()),
        tc.draw(gs::integers::<u64>()),
        tc.draw(gs::integers::<u64>()),
    )
}

fn new_header(kind: &str) -> Header {
    match kind {
        "gnu" => Header::new_gnu(),
        "ustar" => Header::new_ustar(),
        _ => Header::new_old(),
    }
}

#[hegel::test(report_multiple_failures = true)]
fn header_numeric_u64_fields(tc: TestCase) {
    let size = tc.draw(gs::integers::<u64>());
    let mtime = tc.draw(gs::integers::<u64>());
    let atime = tc.draw(gs::integers::<u64>());
    let ctime = tc.draw(gs::integers::<u64>());
    let real_size = tc.draw(gs::integers::<u64>());

    for kind in ["gnu", "ustar", "old"] {
        let mut header = new_header(kind);
        header.set_size(size);
        assert_eq!(header.size().unwrap(), size, "{kind} size");
        header.set_mtime(mtime);
        assert_eq!(header.mtime().unwrap(), mtime, "{kind} mtime");
        if kind == "gnu" {
            let gnu = header.as_gnu_mut().unwrap();
            gnu.set_atime(atime);
            gnu.set_ctime(ctime);
            gnu.set_real_size(real_size);
            assert_eq!(gnu.atime().unwrap(), atime, "GNU atime");
            assert_eq!(gnu.ctime().unwrap(), ctime, "GNU ctime");
            assert_eq!(gnu.real_size().unwrap(), real_size, "GNU real size");
        }
    }
}

#[hegel::test(report_multiple_failures = true)]
fn header_numeric_ids_within_supported_range(tc: TestCase) {
    let maximum = (1u64 << 63) - 1;
    let uid = tc.draw(gs::integers::<u64>().max_value(maximum));
    let gid = tc.draw(gs::integers::<u64>().max_value(maximum));
    for kind in ["gnu", "ustar", "old"] {
        let mut header = new_header(kind);
        header.set_uid(uid);
        assert_eq!(header.uid().unwrap(), uid, "{kind} uid");
        header.set_gid(gid);
        assert_eq!(header.gid().unwrap(), gid, "{kind} gid");
    }
}

#[hegel::test(report_multiple_failures = true)]
fn header_numeric_octal_fields_within_supported_range(tc: TestCase) {
    let maximum = 2_097_151;
    let mode = tc.draw(gs::integers::<u32>().max_value(maximum));
    let major = tc.draw(gs::integers::<u32>().max_value(maximum));
    let minor = tc.draw(gs::integers::<u32>().max_value(maximum));
    for kind in ["gnu", "ustar", "old"] {
        let mut header = new_header(kind);
        header.set_mode(mode);
        assert_eq!(header.mode().unwrap(), mode, "{kind} mode");
        if kind != "old" {
            header.set_device_major(major).unwrap();
            assert_eq!(
                header.device_major().unwrap(),
                Some(major),
                "{kind} device major"
            );
            header.set_device_minor(minor).unwrap();
            assert_eq!(
                header.device_minor().unwrap(),
                Some(minor),
                "{kind} device minor"
            );
        }
    }
}

#[hegel::test(report_multiple_failures = true)]
#[hegel::reproduce_failure("AXiclcihDQAACMTAIvmpGR2CAP01l1RMCpbkqr8mDTNxAQE=")]
#[ignore = "fails: uid 2^63 is read back as zero (https://github.com/gpu004/tar-rs/issues/4)"]
fn header_numeric_uid_overflow(tc: TestCase) {
    let (_, uid, _, _, _, _, _, _, _, _) = draw_full_range(&tc);
    let mut actual = Vec::new();
    for kind in ["gnu", "ustar", "old"] {
        let mut header = new_header(kind);
        header.set_uid(uid);
        actual.push(header.uid().unwrap());
    }
    assert_eq!(actual, vec![uid; 3]);
}

#[hegel::test(report_multiple_failures = true)]
#[hegel::reproduce_failure("AXichcmhDQAACMTAIvmpGR0UIUF8zYmKScEl2epNSwMwEQEB")]
#[ignore = "fails: gid 2^63 is read back as zero (https://github.com/gpu004/tar-rs/issues/4)"]
fn header_numeric_gid_overflow(tc: TestCase) {
    let (_, _, gid, _, _, _, _, _, _, _) = draw_full_range(&tc);
    let mut actual = Vec::new();
    for kind in ["gnu", "ustar", "old"] {
        let mut header = new_header(kind);
        header.set_gid(gid);
        actual.push(header.gid().unwrap());
    }
    assert_eq!(actual, vec![gid; 3]);
}

#[hegel::test(report_multiple_failures = true)]
#[hegel::reproduce_failure("AXicfcqhEQAADMJAenVM0f2nLIeHmDchFAeZNdcn8xiZAJs=")]
#[ignore = "fails: mode 2^21 is read back as zero (https://github.com/gpu004/tar-rs/issues/4)"]
fn header_numeric_mode_overflow(tc: TestCase) {
    let (_, _, _, _, mode, _, _, _, _, _) = draw_full_range(&tc);
    let mut actual = Vec::new();
    for kind in ["gnu", "ustar", "old"] {
        let mut header = new_header(kind);
        header.set_mode(mode);
        actual.push(header.mode().unwrap());
    }
    assert_eq!(actual, vec![mode; 3]);
}

#[hegel::test(report_multiple_failures = true)]
#[hegel::reproduce_failure("AXichcoxEQAADMJAet1QUf8qyyEAsvwSQnFQWXNpehfBAJs=")]
#[ignore = "fails: device major 2^21 is read back as zero in GNU and UStar (https://github.com/gpu004/tar-rs/issues/4)"]
fn header_numeric_device_major_overflow(tc: TestCase) {
    let (_, _, _, _, _, major, _, _, _, _) = draw_full_range(&tc);
    let mut actual = Vec::new();
    for kind in ["gnu", "ustar"] {
        let mut header = new_header(kind);
        header.set_device_major(major).unwrap();
        actual.push(header.device_major().unwrap());
    }
    assert_eq!(actual, vec![Some(major); 2]);
}

#[hegel::test(report_multiple_failures = true)]
#[hegel::reproduce_failure("AXicjcmhEQAADMJAenVM0f2nLIdGEPMihOKgZc3F+xbpAJs=")]
#[ignore = "fails: device minor 2^21 is read back as zero in GNU and UStar (https://github.com/gpu004/tar-rs/issues/4)"]
fn header_numeric_device_minor_overflow(tc: TestCase) {
    let (_, _, _, _, _, _, minor, _, _, _) = draw_full_range(&tc);
    let mut actual = Vec::new();
    for kind in ["gnu", "ustar"] {
        let mut header = new_header(kind);
        header.set_device_minor(minor).unwrap();
        actual.push(header.device_minor().unwrap());
    }
    assert_eq!(actual, vec![Some(minor); 2]);
}
