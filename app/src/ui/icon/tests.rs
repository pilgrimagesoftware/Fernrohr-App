//! `resource-kind-icons` 2.1-2.2: every bundled icon file loads, and each
//! kind resolves to the right one.

use super::{KindIcon, for_container, for_kind};
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

/// 2.2: core-group kinds.
#[test]
fn core_kinds_get_their_own_icons() {
    assert_eq!(for_kind("", "Pod"), KindIcon::Pod);
    assert_eq!(for_kind("", "ConfigMap"), KindIcon::ConfigMap);
    assert_eq!(for_kind("", "Secret"), KindIcon::Secret);
    assert_eq!(for_kind("", "Service"), KindIcon::Service);
    assert_eq!(for_kind("", "Node"), KindIcon::Node);
}

/// 2.2: a kind in a named built-in group.
#[test]
fn apps_and_other_builtin_group_kinds_get_their_own_icons() {
    assert_eq!(for_kind("apps", "Deployment"), KindIcon::Deployment);
    assert_eq!(for_kind("apps", "StatefulSet"), KindIcon::StatefulSet);
    assert_eq!(for_kind("batch", "CronJob"), KindIcon::CronJob);
    assert_eq!(for_kind("networking.k8s.io", "Ingress"), KindIcon::Ingress);
    assert_eq!(
        for_kind("rbac.authorization.k8s.io", "ClusterRoleBinding"),
        KindIcon::ClusterRoleBinding
    );
    assert_eq!(
        for_kind("apiextensions.k8s.io", "CustomResourceDefinition"),
        KindIcon::CustomResourceDefinition
    );
}

/// 2.2: the group is part of the key. A CRD named like a built-in kind gets
/// the custom-resource icon, and a built-in kind in the wrong group isn't
/// matched by name alone.
#[test]
fn a_crd_named_like_a_builtin_kind_is_a_custom_resource() {
    assert_eq!(for_kind("example.com", "Pod"), KindIcon::CustomResource);
    assert_eq!(
        for_kind("ferns.example.com", "Deployment"),
        KindIcon::CustomResource
    );
    assert_eq!(for_kind("apps", "Pod"), KindIcon::Kind);
}

/// CRD groups under `k8s.io` - the Gateway API, volume snapshots - are custom
/// resources too, not built-in kinds.
#[test]
fn community_crds_under_k8s_io_are_custom_resources() {
    assert_eq!(
        for_kind("gateway.networking.k8s.io", "Gateway"),
        KindIcon::CustomResource
    );
    assert_eq!(
        for_kind("snapshot.storage.k8s.io", "VolumeSnapshot"),
        KindIcon::CustomResource
    );
}

/// A built-in kind the set doesn't cover gets the generic kind icon, not the
/// custom-resource one.
#[test]
fn an_uncovered_builtin_kind_gets_the_generic_icon() {
    assert_eq!(
        for_kind("discovery.k8s.io", "EndpointSlice"),
        KindIcon::Kind
    );
    assert_eq!(for_kind("", "Event"), KindIcon::Kind);
    assert_eq!(for_kind("coordination.k8s.io", "Lease"), KindIcon::Kind);
}

/// 2.2: a container.
#[test]
fn a_container_gets_the_container_icon() {
    assert_eq!(for_container(), KindIcon::Container);
}
