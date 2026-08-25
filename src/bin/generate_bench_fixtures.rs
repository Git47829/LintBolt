use std::fs;
use std::path::Path;

const SMALL_BYTES: usize = 16 * 1024;
const MIXED_BYTES: usize = 512 * 1024;
const LARGE_BYTES: usize = 16 * 1024 * 1024;
const CLOSING: &[u8] = b"</body></html>\n";
const LARGE_CLOSING: &[u8] = b"</tbody></table></main></body></html>\n";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = Path::new("benchmarks/fixtures");
    fs::create_dir_all(directory)?;

    let small = small_fixture();
    let mixed = mixed_fixture();
    let large = large_fixture();

    assert_fixture(&small, SMALL_BYTES, 0);
    assert_fixture(&mixed, MIXED_BYTES, 256);
    assert_fixture(&large, LARGE_BYTES, 0);

    fs::write(directory.join("agent-clean-small.html"), small)?;
    fs::write(directory.join("agent-mixed-repair.html"), mixed)?;
    fs::write(directory.join("agent-report-large.html"), large)?;
    println!("generated exact-size fixtures: {SMALL_BYTES}, {MIXED_BYTES}, {LARGE_BYTES} bytes");
    Ok(())
}

fn small_fixture() -> Vec<u8> {
    let mut out = header("Small clean agent artifact");
    let row = b"<article><h2>Artifact card</h2><img src=preview.webp alt=\"Artifact preview\" width=320 height=180><button type=button>Inspect</button></article>\n";
    fill_with_rows(&mut out, row, SMALL_BYTES, CLOSING);
    out
}

fn mixed_fixture() -> Vec<u8> {
    let mut out = format!(
        "<!doctype html>\n<html lang=en><head><meta charset=utf-8><title>{}</title>\n",
        "Mixed repair artifact"
    )
    .into_bytes();
    out.extend_from_slice(b"<style>.card{display:grid}.quiet{color:#555}</style>\n");
    out.extend_from_slice(
        b"<script>const marker = '</not-a-real-end-tag>'; window.__artifactReady = true;</script>\n",
    );
    out.extend_from_slice(b"</head><body>\n");
    for index in 0..256 {
        out.extend_from_slice(
            format!(
                "<section class=card data-index={index}><h2>Generated item {index}</h2><img src=\"asset-{index}.webp\" width=320 height=180><button type=button>Repair</button><iframe title=\"Item {index} details\" src=about:blank></iframe></section>\n"
            )
            .as_bytes(),
        );
    }
    let row = b"<section class=quiet><p>Already repaired content with entities &amp; text.</p><img src=ok.webp alt=\"Complete preview\"><button type=button>Keep</button></section>\n";
    fill_with_rows(&mut out, row, MIXED_BYTES, CLOSING);
    out
}

fn large_fixture() -> Vec<u8> {
    let mut out = header("Large generated report");
    out.extend_from_slice(b"<main id=report><table><thead><tr><th>Artifact</th><th>Status</th><th>Preview</th></tr></thead><tbody>\n");
    let row = b"<tr><td>Generated HTML artifact</td><td data-state=verified>verified</td><td><img src=thumb.webp alt=\"Verified artifact preview\" width=96 height=54></td></tr>\n";
    fill_with_rows(&mut out, row, LARGE_BYTES, LARGE_CLOSING);
    out
}

fn header(title: &str) -> Vec<u8> {
    format!(
        "<!doctype html>\n<html lang=en><head><meta charset=utf-8><title>{title}</title></head><body>\n"
    )
    .into_bytes()
}

fn fill_with_rows(out: &mut Vec<u8>, row: &[u8], target: usize, closing: &[u8]) {
    let reserve = closing.len() + 7;
    while out.len() + row.len() + reserve <= target {
        out.extend_from_slice(row);
    }
    let padding = target - out.len() - closing.len();
    assert!(
        padding >= 7,
        "fixture row leaves too little room for padding"
    );
    out.extend_from_slice(b"<!--");
    out.resize(out.len() + padding - 7, b' ');
    out.extend_from_slice(b"-->");
    out.extend_from_slice(closing);
    assert_eq!(out.len(), target);
}

fn assert_fixture(input: &[u8], expected_bytes: usize, expected_missing_alt: usize) {
    assert_eq!(input.len(), expected_bytes);
    let tags = input
        .windows(5)
        .filter(|window| *window == b"<img ")
        .count();
    let alt_attributes = input.windows(4).filter(|window| *window == b"alt=").count();
    assert_eq!(tags - alt_attributes, expected_missing_alt);
}
