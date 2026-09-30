// `use super::*` here would re-import `gpui_kit`'s `test` attribute macro (this file's
// `use gpui_kit::*` brings it in), shadowing the builtin `#[test]` and sending a plain
// sync test into `#[gpui_kit::test]`'s async-runtime expansion instead - hence the
// explicit imports below rather than a glob.
use super::{
    CARD_CHROME_HEIGHT, CARD_LIST_MAX_HEIGHT, CARD_WIDTH, LOGO_HEIGHT, LOGO_WIDTH, MIN_WINDOW_SIZE,
    PICKER_CONTENT_GAP,
};
use crate::k8s::cluster::kubeconfig;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

static COUNTER: AtomicU64 = AtomicU64::new(0);

const FIXTURE: &str = r#"
apiVersion: v1
kind: Config
clusters:
  - name: kind-dev
    cluster:
      server: https://127.0.0.1:6443
contexts:
  - name: kind-dev
    context:
      cluster: kind-dev
      user: kind-dev
current-context: kind-dev
users:
  - name: kind-dev
    user: {}
"#;

fn fixture_path() -> std::path::PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("fernrohr-picker-fixture-{n}.yaml"));
    fs::write(&path, FIXTURE).unwrap();
    path
}

#[test]
fn lists_contexts_from_a_valid_kubeconfig() {
    let path = fixture_path();
    let contexts = kubeconfig::list_context_names(Some(&path)).unwrap();
    assert_eq!(contexts, vec!["kind-dev".to_string()]);
}

#[test]
fn missing_kubeconfig_is_reported_as_an_error() {
    let missing = std::env::temp_dir().join("fernrohr-picker-fixture-does-not-exist.yaml");
    let result = kubeconfig::list_context_names(Some(&missing));
    assert!(result.is_err());
}

/// Guards the `include_bytes!` in [`super::logo`]: a truncated or placeholder
/// asset compiles fine and only fails as a blank gap in the UI. WebP's RIFF
/// header carries the total file size, so a length that disagrees with it
/// catches truncation exactly rather than by proxy.
#[test]
fn the_embedded_logo_is_a_complete_webp() {
    let bytes = include_bytes!("../../../assets/fernrohr-logo.webp");
    assert!(bytes.starts_with(b"RIFF"), "missing RIFF signature");
    assert_eq!(&bytes[8..12], b"WEBP", "RIFF payload is not WebP");

    let declared = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    assert_eq!(
        declared + 8,
        bytes.len(),
        "RIFF size disagrees with file length - asset looks truncated"
    );

    // The lossless full-resolution encode is ~1.14MB. The floor catches a
    // placeholder; the ceiling catches either the 1.6MB brand source in
    // `images/` or a lossy re-encode being committed here by mistake - this
    // asset is required to be bit-identical to the master, and both of those
    // are not.
    assert!(
        (1_000_000..1_400_000).contains(&bytes.len()),
        "logo is {} bytes, outside the expected lossless full-resolution range",
        bytes.len()
    );
}

/// The window minimum exists so the picker's centred column never overflows and
/// clips the logo. These pin the composition it is derived from, and catch the
/// one mistake derivation cannot catch on its own: a minimum *larger* than the
/// default window, which would open every fresh window already violating its own
/// floor.
#[test]
fn the_window_minimum_fits_the_picker() {
    let min_width = f32::from(MIN_WINDOW_SIZE.width);
    let min_height = f32::from(MIN_WINDOW_SIZE.height);

    assert!(
        min_width >= CARD_WIDTH,
        "minimum width {min_width} is narrower than the {CARD_WIDTH}px card"
    );

    let required = LOGO_HEIGHT + PICKER_CONTENT_GAP + CARD_CHROME_HEIGHT + CARD_LIST_MAX_HEIGHT;
    assert!(
        min_height >= required,
        "minimum height {min_height} cannot fit the picker, which needs {required}"
    );

    let default_layout = crate::config::workspace::WindowLayout::default();
    assert!(
        min_width <= default_layout.width && min_height <= default_layout.height,
        "minimum {}x{} exceeds the default window {}x{} - a fresh window would open \
         already violating its own minimum",
        min_width,
        min_height,
        default_layout.width,
        default_layout.height
    );
}

/// The asset is re-encoded at full source resolution precisely so that rendering it
/// at `LOGO_WIDTH` stays sharp on a Retina display, and `LOGO_HEIGHT` is typed to its
/// aspect so the mark is not stretched. Neither link is enforced at runtime, so read
/// the real canvas out of the shipped bytes and hold both.
#[test]
fn the_embedded_logo_is_large_enough_for_the_size_it_renders_at() {
    /// Canvas dimensions of `app/assets/fernrohr-logo.webp`. A lossless WebP is a
    /// single `VP8L` chunk - unlike the lossy-with-alpha `VP8X` extended format -
    /// and packs the canvas into a 32-bit field: 12 bytes of RIFF header, then the
    /// chunk id and its length, then a 0x2f signature byte, then width-1 in bits
    /// 0-13 and height-1 in bits 14-27.
    fn canvas_size() -> (usize, usize) {
        let bytes = include_bytes!("../../../assets/fernrohr-logo.webp");
        assert_eq!(&bytes[12..16], b"VP8L", "expected a lossless (VP8L) WebP");
        assert_eq!(bytes[20], 0x2f, "missing the VP8L signature byte");
        let packed = u32::from_le_bytes(bytes[21..25].try_into().unwrap());
        (
            (packed & 0x3fff) as usize + 1,
            ((packed >> 14) & 0x3fff) as usize + 1,
        )
    }

    let (width, height) = canvas_size();
    assert!(
        width >= (2. * LOGO_WIDTH) as usize,
        "asset is {width}px wide but renders at {LOGO_WIDTH}px - soft on Retina"
    );

    let asset_aspect = width as f32 / height as f32;
    let rendered_aspect = LOGO_WIDTH / LOGO_HEIGHT;
    assert!(
        (asset_aspect - rendered_aspect).abs() < 0.01,
        "asset aspect {asset_aspect} does not match the rendered {rendered_aspect} - \
         the mark would be stretched"
    );
}

/// Section 3.2: drives `ClusterPicker` into a fake `Failed` attempt directly (via
/// `ClusterConnection::test_with_state`, no real connect) rather than through
/// `select`, so the failure path doesn't depend on network access or a real
/// kubeconfig - then confirms the picker remains interactive by driving a second
/// `select` call afterward.
///
/// The retry also goes through the stub connection factory rather than
/// `ClusterRegistry::connection`: a real connect spawns tokio work on a runtime
/// worker thread, which gpui's test scheduler flags as nondeterminism and turns
/// into a flaky failure. This test previously failed that way on `develop`.
#[gpui_kit::test]
async fn failed_attempt_shows_the_reason_and_stays_interactive(cx: &mut gpui_kit::TestAppContext) {
    use super::{Attempt, ClusterPicker};
    use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
    use gpui_kit::{AppContext as _, Entity};

    /// Stands in for `ClusterRegistry::connection`: hands back a connection in a
    /// non-terminal state so selecting again replaces the attempt without any
    /// real I/O.
    fn stub_connection(cx: &mut gpui_kit::App, _context_name: &str) -> Entity<ClusterConnection> {
        cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connecting))
    }

    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let window = cx.add_window(ClusterPicker::new);

    window
        .update(cx, |picker, _window, cx| {
            picker.connection_factory = Some(stub_connection);
            picker.attempt = Some(Attempt {
                context_name: "kind-dev".to_string(),
                connection: cx.new(|_| {
                    ClusterConnection::test_with_state(ConnectionState::Failed(
                        "connection refused".to_string(),
                    ))
                }),
                connected: false,
            });
            cx.notify();
        })
        .unwrap();

    window
        .update(cx, |picker, _window, cx| {
            let attempt = picker.attempt.as_ref().expect("attempt is still set");
            assert_eq!(attempt.context_name, "kind-dev");
            assert!(matches!(
                attempt.connection.read(cx).state,
                ConnectionState::Failed(ref reason) if reason == "connection refused"
            ));
        })
        .unwrap();

    // Stays interactive: a failed attempt doesn't leave the picker stuck - selecting
    // again (retry, or a different context) starts a fresh attempt.
    window
        .update(cx, |picker, _window, cx| {
            picker.select("kind-dev".to_string(), cx)
        })
        .unwrap();
    cx.run_until_parked();

    window
        .update(cx, |picker, _window, _cx| {
            assert_eq!(
                picker
                    .attempt
                    .as_ref()
                    .expect("select started a new attempt")
                    .context_name,
                "kind-dev"
            );
        })
        .unwrap();
}

/// Tasks.md 3.1: selecting a tunnel for a row writes through `TunnelStore::bind`
/// (asserted at the file, since `tunnels.toml` is the only state) and updates the
/// cached binding the row's label reads (`context_row`/`picker_tunnel::bound_label`
/// turn that into text - covered on their own in `picker_tunnel.rs`'s tests);
/// selecting Direct (`None`) unbinds it again.
#[gpui_kit::test]
async fn set_tunnel_binds_and_unbinds_through_the_store(cx: &mut gpui_kit::TestAppContext) {
    use super::ClusterPicker;
    use crate::config::tunnels::TunnelAuth;
    use crate::tunnel::store::TunnelStore;

    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });

    let tunnels_path = std::env::temp_dir().join(format!(
        "fernrohr-picker-set-tunnel-test-{}.toml",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&tunnels_path);
    let store = TunnelStore::new(tunnels_path.clone());
    store
        .create(
            "qa-bastion",
            crate::config::tunnels::TunnelConfig {
                name: "QA".into(),
                bastion_user: "ops".into(),
                bastion_host: "bastion.example.com".into(),
                bastion_port: 22,
                jump_hosts: Vec::new(),
                auth: TunnelAuth::default(),
            },
            None,
        )
        .unwrap();

    let window = cx.add_window(ClusterPicker::new);
    window
        .update(cx, |picker, _window, _cx| {
            // Test-only override: a fresh `ClusterPicker` otherwise reads this
            // machine's real `tunnels.toml`, which this test must not touch.
            picker.tunnels_path = tunnels_path.clone();
        })
        .unwrap();

    window
        .update(cx, |picker, _window, cx| {
            picker.set_tunnel("kind-dev", Some("qa-bastion".to_string()), cx);
        })
        .unwrap();
    assert_eq!(
        TunnelStore::new(tunnels_path.clone()).binding_for("kind-dev"),
        Some("qa-bastion".to_string())
    );
    window
        .update(cx, |picker, _window, _cx| {
            assert_eq!(
                picker.tunnel_bindings.get("kind-dev"),
                Some(&"qa-bastion".to_string()),
                "the cached binding a row's label reads must update"
            );
        })
        .unwrap();

    window
        .update(cx, |picker, _window, cx| {
            picker.set_tunnel("kind-dev", None, cx);
        })
        .unwrap();
    assert_eq!(
        TunnelStore::new(tunnels_path.clone()).binding_for("kind-dev"),
        None,
        "selecting Direct must remove the binding"
    );
    window
        .update(cx, |picker, _window, _cx| {
            assert!(!picker.tunnel_bindings.contains_key("kind-dev"));
        })
        .unwrap();

    let _ = std::fs::remove_file(&tunnels_path);
}

/// A tunnel created elsewhere (the Tunnels window) reaches an already-open picker's
/// row dropdown once the write bumps `TunnelsRevision`, without the picker writing
/// anything itself.
#[gpui_kit::test]
async fn a_tunnel_created_elsewhere_appears_in_an_open_picker(cx: &mut gpui_kit::TestAppContext) {
    use super::ClusterPicker;
    use crate::config::tunnels::{TunnelAuth, TunnelConfig};
    use crate::tunnel::store::TunnelStore;

    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::runtime::init(cx);
    });
    let tunnels_path = std::env::temp_dir().join(format!(
        "fernrohr-picker-revision-test-{}.toml",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&tunnels_path);

    let window = cx.add_window(ClusterPicker::new);
    window
        .update(cx, |picker, _window, _cx| {
            // Test-only override, as above: never read this machine's real file.
            picker.tunnels_path = tunnels_path.clone();
            picker.tunnel_choices.clear();
        })
        .unwrap();

    TunnelStore::new(tunnels_path.clone())
        .create(
            "qa-bastion",
            TunnelConfig {
                name: "QA".into(),
                bastion_user: "ops".into(),
                bastion_host: "bastion.example.com".into(),
                bastion_port: 22,
                jump_hosts: Vec::new(),
                auth: TunnelAuth::default(),
            },
            None,
        )
        .unwrap();
    cx.update(crate::ui::tunnels::notify_tunnels_changed);
    cx.run_until_parked();

    window
        .update(cx, |picker, _window, _cx| {
            assert!(
                picker
                    .tunnel_choices
                    .iter()
                    .any(|choice| choice.id == "qa-bastion"),
                "the new tunnel should be offered without reopening the picker"
            );
        })
        .unwrap();
    let _ = std::fs::remove_file(&tunnels_path);
}
