//! `resource-kind-icons` 2.1: every bundled icon file loads.

use super::KindIcon;
use gpui_kit::TestAppContext;
use std::collections::HashSet;
use std::path::Path;

/// Parses and rasterizes `svg` with GPUI's own SVG renderer at 2x, returning
/// its BGRA pixels. Panics, naming `what`, if it doesn't parse or is empty.
fn render(cx: &mut TestAppContext, what: &str, svg: &[u8]) -> Vec<u8> {
    cx.update(|cx| {
        let image = cx
            .svg_renderer()
            .render_single_frame(svg, 2.0)
            .unwrap_or_else(|error| panic!("{what} doesn't parse: {error}"));
        let size = image.size(0);
        assert!(
            size.width.0 > 0 && size.height.0 > 0,
            "{what} renders empty"
        );
        image.as_bytes(0).expect("one frame").to_vec()
    })
}

/// Whether any pixel is clearly blue - the set's `#326ce5` fill - so an icon
/// that rendered as a flat mask or a blank would fail.
fn has_blue(bgra: &[u8]) -> bool {
    bgra.as_chunks::<4>()
        .0
        .iter()
        .any(|&[b, g, r, a]| a > 200 && b > 180 && r < 120 && g < 160)
}

/// 2.1: every icon file this app ships - each `KindIcon`'s embedded SVG, and
/// every file in the two asset directories, used or not - parses and
/// rasterizes, in full colour.
#[gpui_kit::test]
fn every_bundled_icon_loads_and_renders_in_colour(cx: &mut TestAppContext) {
    for icon in KindIcon::ALL {
        let pixels = render(cx, &format!("{icon:?}"), icon.svg());
        assert!(has_blue(&pixels), "{icon:?} renders without its blue fill");
    }

    let assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icons");
    let mut files = 0;
    for dir in ["kubernetes", "fallback"] {
        for entry in std::fs::read_dir(assets.join(dir)).expect("the icon directory exists") {
            let path = entry.expect("a readable entry").path();
            if path.extension().is_some_and(|ext| ext == "svg") {
                let bytes = std::fs::read(&path).expect("a readable icon");
                render(cx, &path.display().to_string(), &bytes);
                files += 1;
            }
        }
    }
    assert!(
        files >= KindIcon::ALL.len(),
        "only {files} icon files found"
    );
}

/// Each variant is its own file, so no two icons are accidentally the same
/// artwork.
#[test]
fn every_icon_is_a_distinct_file() {
    let distinct: HashSet<&[u8]> = KindIcon::ALL.iter().map(|icon| icon.svg()).collect();
    assert_eq!(distinct.len(), KindIcon::ALL.len());
}
