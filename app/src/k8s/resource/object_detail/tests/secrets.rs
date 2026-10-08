//! A Secret's values never reach the panel (`resource-links` 6.2,
//! `object-detail`'s "Secret values are never shown").

use super::fixtures::{kind, object, stub_panel, target};
use super::sections::field;
use crate::k8s::resource::object_detail::redact::LAST_APPLIED_ANNOTATION;
use gpui_kit::TestAppContext;
use jiff::Timestamp;
use serde_json::json;

const PASSWORD: &str = "hunter2-hunter2";
const PASSWORD_B64: &str = "aHVudGVyMi1odW50ZXIy";
const TOKEN: &str = "plain-token-value";

fn secret() -> kube::api::DynamicObject {
    object(json!({
        "apiVersion": "v1",
        "kind": "Secret",
        "type": "Opaque",
        "metadata": {
            "name": "app-secrets",
            "namespace": "staging",
            "annotations": {
                LAST_APPLIED_ANNOTATION: format!(
                    "{{\"stringData\":{{\"token\":\"{TOKEN}\"}},\"data\":{{\"password\":\"{PASSWORD_B64}\"}}}}"
                ),
                "team": "platform",
            },
        },
        "data": { "password": PASSWORD_B64 },
        "stringData": { "token": TOKEN },
    }))
}

/// Neither the structured view's rows nor the YAML contain any value - not
/// the encoded one, not the decoded one, not the one in the last-applied
/// annotation - and each key's size is right.
#[gpui_kit::test]
async fn no_secret_value_reaches_either_view(cx: &mut TestAppContext) {
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
    });
    let secrets = kind("", "v1", "Secret", true);
    let (window, panel) = stub_panel(
        cx,
        target(secrets.clone(), Some("staging"), "app-secrets"),
        vec![secrets],
    );
    window
        .update(cx, |_, _, cx| {
            panel.update(cx, |panel, cx| panel.test_set_loaded(secret(), cx))
        })
        .unwrap();

    let (sections, yaml) = window
        .update(cx, |_, _, cx| {
            let panel = panel.read(cx);
            (
                panel.sections(Timestamp::from_second(0).unwrap()),
                panel.yaml(),
            )
        })
        .unwrap();
    let yaml = yaml.expect("the secret loaded");
    let rendered_rows = format!("{sections:?}");

    for value in [PASSWORD, PASSWORD_B64, TOKEN] {
        assert!(!yaml.contains(value), "YAML leaks {value}:\n{yaml}");
        assert!(!rendered_rows.contains(value), "fields leak {value}");
    }
    assert_eq!(field(&sections, "Type").value.text(), "Opaque");
    assert_eq!(
        field(&sections, "Data").value.text(),
        format!("password: {} bytes", PASSWORD.len())
    );
    assert_eq!(
        field(&sections, "String Data").value.text(),
        format!("token: {} bytes", TOKEN.len())
    );
    assert!(
        yaml.contains("team: platform"),
        "other annotations are left alone"
    );
}

/// 3.1: the object viewer reveals one Secret value by Tab and Space, keeps
/// the YAML redacted while it's shown, never prints it, and `h` hides it.
#[gpui_kit::test]
async fn a_value_is_revealed_by_keyboard_and_the_yaml_stays_redacted(cx: &mut TestAppContext) {
    use super::fixtures::{Host, Route, serve};
    use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
    use crate::k8s::resource::object_detail::ObjectDetailPanel;
    use crate::k8s::resource::secret_value::Reveal;
    use crate::ui::nav::NavTarget;
    use crate::ui::panel_title::PanelScope;
    use gpui_kit::component::Root;
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{AppContext as _, ElementId, VisualTestContext};

    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        let mut registry = crate::command::CommandRegistry::new();
        crate::k8s::resource::object_detail::register_commands(&mut registry);
        let bindings = crate::keymap::bindings(
            &registry,
            &crate::keymap::KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
    });
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let (addr, _server) = handle.block_on(serve(vec![Route {
        path: "/api/v1/namespaces/staging/secrets/app-secrets ",
        status: "200 OK",
        body: serde_json::to_value(secret()).unwrap(),
    }]));
    let client = {
        let _guard = handle.enter();
        kube::Client::try_from(kube::Config::new(format!("http://{addr}").parse().unwrap()))
            .unwrap()
    };
    let connection = cx.update(|cx| {
        cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connected(client)))
    });
    let secrets = kind("", "v1", "Secret", true);
    let object = target(secrets.clone(), Some("staging"), "app-secrets");
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::Object(object.clone()), "kind-dev".into());
        let panel = cx.new(|cx| {
            ObjectDetailPanel::with_connection(object, scope, connection, vec![secrets], cx)
        });
        built = Some(panel.clone());
        let host = cx.new(|_| Host { panel });
        Root::new(host, window, cx)
    });
    let panel = built.expect("the window built its panel");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let wait = |vcx: &mut VisualTestContext, done: &dyn Fn(&ObjectDetailPanel) -> bool| {
        for _ in 0..400 {
            vcx.run_until_parked();
            if vcx.update(|_, cx| done(panel.read(cx))) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("timed out waiting for the panel");
    };
    wait(&mut vcx, &|panel| panel.yaml().is_some());
    window
        .update(&mut vcx, |_, window, cx| {
            panel.read(cx).focus_handle.clone().focus(window, cx)
        })
        .unwrap();

    // Tab to the password's Show button, then press (and release) Space.
    let button = ElementId::NamedInteger("object-reveal".into(), 0);
    let mut reached = false;
    for _ in 0..20 {
        let focused = vcx
            .update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                window.try_find(button.clone()).and_then(|b| b.focused())
            })
            .unwrap();
        if focused == Some(true) {
            reached = true;
            break;
        }
        vcx.simulate_keystrokes("tab");
        vcx.run_until_parked();
    }
    assert!(reached, "Tab reaches the Show button");
    let space = gpui_kit::Keystroke::parse("space").unwrap();
    vcx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: space.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    vcx.simulate_event(gpui_kit::KeyUpEvent { keystroke: space });
    wait(&mut vcx, &|panel| {
        matches!(panel.revealed.get("password"), Some(Reveal::Shown(_)))
    });

    let (yaml, state) = vcx.update(|_, cx| {
        let panel = panel.read(cx);
        (
            panel.yaml().expect("loaded"),
            format!("{:?}", panel.revealed),
        )
    });
    assert!(
        !yaml.contains(PASSWORD_B64) && !yaml.contains(PASSWORD),
        "YAML leaks while revealed"
    );
    assert!(
        !state.contains(PASSWORD),
        "the reveal state prints a value: {state}"
    );

    vcx.simulate_keystrokes("h");
    vcx.run_until_parked();
    assert!(vcx.update(|_, cx| panel.read(cx).revealed.is_empty()));
}

/// `saved-panel-layouts` design.md D8: `SecretValue` has no `Serialize` impl,
/// so a revealed value can't reach a saved layout through any serialization
/// path - nothing extra needs to redact it. This is a structural regression
/// guard, not a behavioural one: it wraps the panel's own `dump` (the same
/// one a saved layout's `dock` field stores) in a `SavedLayout` and checks
/// neither its `Debug` output nor its JSON contains the fixture value,
/// mirroring this file's own keyboard-reveal test above.
#[gpui_kit::test]
async fn a_revealed_value_stays_out_of_a_saved_layout(cx: &mut TestAppContext) {
    use super::fixtures::{Host, Route, serve};
    use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
    use crate::k8s::resource::object_detail::ObjectDetailPanel;
    use crate::k8s::resource::secret_value::Reveal;
    use crate::ui::nav::NavTarget;
    use crate::ui::panel_title::PanelScope;
    use gpui_kit::component::Root;
    use gpui_kit::component::dock::{BasePanel as _, DockAreaState};
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{AppContext as _, ElementId, VisualTestContext};

    cx.executor().allow_parking();
    cx.update(|cx| {
        crate::util::test_ui::init(cx);
        crate::runtime::init(cx);
        let mut registry = crate::command::CommandRegistry::new();
        crate::k8s::resource::object_detail::register_commands(&mut registry);
        let bindings = crate::keymap::bindings(
            &registry,
            &crate::keymap::KeymapConfig::default(),
            cx.keyboard_mapper().as_ref(),
        );
        cx.bind_keys(bindings);
    });
    let handle = cx.update(|cx| crate::runtime::handle(cx));
    let (addr, _server) = handle.block_on(serve(vec![Route {
        path: "/api/v1/namespaces/staging/secrets/app-secrets ",
        status: "200 OK",
        body: serde_json::to_value(secret()).unwrap(),
    }]));
    let client = {
        let _guard = handle.enter();
        kube::Client::try_from(kube::Config::new(format!("http://{addr}").parse().unwrap()))
            .unwrap()
    };
    let connection = cx.update(|cx| {
        cx.new(|_| ClusterConnection::test_with_state(ConnectionState::Connected(client)))
    });
    let secrets = kind("", "v1", "Secret", true);
    let object = target(secrets.clone(), Some("staging"), "app-secrets");
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let scope = PanelScope::new(NavTarget::Object(object.clone()), "kind-dev".into());
        let panel = cx.new(|cx| {
            ObjectDetailPanel::with_connection(object, scope, connection, vec![secrets], cx)
        });
        built = Some(panel.clone());
        let host = cx.new(|_| Host { panel });
        Root::new(host, window, cx)
    });
    let panel = built.expect("the window built its panel");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let wait = |vcx: &mut VisualTestContext, done: &dyn Fn(&ObjectDetailPanel) -> bool| {
        for _ in 0..400 {
            vcx.run_until_parked();
            if vcx.update(|_, cx| done(panel.read(cx))) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        panic!("timed out waiting for the panel");
    };
    wait(&mut vcx, &|panel| panel.yaml().is_some());
    window
        .update(&mut vcx, |_, window, cx| {
            panel.read(cx).focus_handle.clone().focus(window, cx)
        })
        .unwrap();

    let button = ElementId::NamedInteger("object-reveal".into(), 0);
    let mut reached = false;
    for _ in 0..20 {
        let focused = vcx
            .update_window(window.into(), |_, window, cx| {
                window.render_frame(cx);
                window.try_find(button.clone()).and_then(|b| b.focused())
            })
            .unwrap();
        if focused == Some(true) {
            reached = true;
            break;
        }
        vcx.simulate_keystrokes("tab");
        vcx.run_until_parked();
    }
    assert!(reached, "Tab reaches the Show button");
    let space = gpui_kit::Keystroke::parse("space").unwrap();
    vcx.simulate_event(gpui_kit::KeyDownEvent {
        keystroke: space.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    vcx.simulate_event(gpui_kit::KeyUpEvent { keystroke: space });
    wait(&mut vcx, &|panel| {
        matches!(panel.revealed.get("password"), Some(Reveal::Shown(_)))
    });

    let dump = vcx.update(|_, cx| panel.read(cx).dump(cx));
    let saved_layout = crate::config::saved_layouts::SavedLayout {
        version: crate::consts::SAVED_LAYOUT_SCHEMA_VERSION,
        name: "test".into(),
        created_at: "1970-01-01T00:00:00Z".into(),
        updated_at: "1970-01-01T00:00:00Z".into(),
        contexts: vec!["kind-dev".into()],
        dock: DockAreaState {
            version: None,
            center: dump,
            left_dock: None,
            right_dock: None,
            bottom_dock: None,
        },
        resource_panel_width: Some(240.0),
        window_width: 1200.0,
        window_height: 800.0,
    };
    let debug = format!("{saved_layout:?}");
    let json = serde_json::to_string(&saved_layout).expect("a saved layout serializes");
    for value in [PASSWORD, PASSWORD_B64] {
        assert!(
            !debug.contains(value),
            "the saved layout's Debug leaks {value}"
        );
        assert!(
            !json.contains(value),
            "the saved layout's JSON leaks {value}"
        );
    }
}
