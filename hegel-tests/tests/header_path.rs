mod common;

use common::draw_path;
use hegel::TestCase;
use tar::Header;

#[hegel::test(report_multiple_failures = true)]
fn header_path(tc: TestCase) {
    let path = draw_path(&tc, 8, 40);
    let expected = path.as_bytes();

    for kind in ["gnu", "ustar", "old"] {
        let mut header = match kind {
            "gnu" => Header::new_gnu(),
            "ustar" => Header::new_ustar(),
            _ => Header::new_old(),
        };
        if header.set_path(&path).is_ok() {
            assert_eq!(header.path_bytes().as_ref(), expected, "{kind} path");
        }
        if header.set_link_name(&path).is_ok() {
            assert_eq!(
                header.link_name_bytes().unwrap().as_ref(),
                expected,
                "{kind} link name"
            );
        }
    }
}
