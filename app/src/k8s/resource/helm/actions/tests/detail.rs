//! Rollback and Uninstall from the release detail panel, by keystrokes:
//! History's selection is Rollback's target, Escape cancels, Enter cancels
//! Uninstall and its deliberate shortcut confirms, a failure shows `helm`'s
//! message, and without `helm` nothing asks or runs.

use super::fixture::{CONTEXT, FakeHelm, expected, normalized, start, wait_until};
use crate::helm_cli::{HelmStatus, set_status};
use crate::k8s::resource::helm::ReleaseKey;
use crate::k8s::resource::helm::actions::ActionHost as _;
use crate::k8s::resource::helm::detail::HelmReleasePanel;
use crate::k8s::test_cluster::FakeCluster;
use crate::ui::confirm_dialog::deliver_first_frame;
use crate::ui::nav::NavTarget;
use crate::ui::panel_title::PanelScope;
use gpui_kit::component::Root;
use gpui_kit::component::WindowExt as _;
use gpui_kit::{AppContext as _, Entity, Focusable as _, TestAppContext, VisualTestContext};

struct Harness {
    _cluster: FakeCluster,
    helm: FakeHelm,
    panel: Entity<HelmReleasePanel>,
    vcx: VisualTestContext,
}

/// `web`'s detail panel, focused, its three revisions listed.
fn open(cx: &mut TestAppContext, label: &str, helm_then: &str) -> Harness {
    let (cluster, helm) = start(cx, label, helm_then);
    let key = ReleaseKey {
        namespace: "shop".into(),
        name: "web".into(),
    };
    let scope = PanelScope::new(NavTarget::HelmRelease(key.clone()), CONTEXT.into());
    let mut built = None;
    let window = cx.add_window(|window, cx| {
        let panel = cx.new(|cx| HelmReleasePanel::new(key, scope, cx));
        built = Some(panel.clone());
        Root::new(panel, window, cx)
    });
    let panel = built.expect("the window built its panel");
    let mut vcx = VisualTestContext::from_window(window.into(), cx);
    let focus = panel.clone();
    vcx.update(|window, cx| focus.read(cx).focus_handle(cx).focus(window, cx));
    let listed = panel.clone();
    wait_until(&mut vcx, "listed web's revisions", |vcx| {
        vcx.update(|_, cx| listed.read(cx).history(cx).len() == 3)
    });
    Harness {
        _cluster: cluster,
        helm,
        panel,
        vcx,
    }
}

impl Harness {
    fn press(&mut self, keys: &str) {
        self.vcx.simulate_keystrokes(keys);
        self.vcx.run_until_parked();
    }

    fn dialog_open(&mut self) -> bool {
        self.vcx.update(|window, cx| window.has_active_dialog(cx))
    }

    fn wait_for_dialog(&mut self) {
        wait_until(&mut self.vcx, "asked", |vcx| {
            vcx.update(|window, cx| window.has_active_dialog(cx))
        });
        deliver_first_frame(&mut self.vcx);
    }

    fn rollback_target(&mut self) -> Option<u32> {
        let panel = self.panel.clone();
        self.vcx.update(|_, cx| panel.read(cx).rollback_target(cx))
    }

    fn refusal(&mut self) -> Option<(String, String)> {
        let panel = self.panel.clone();
        self.vcx.update(|_, cx| {
            panel.update(cx, |panel, _| {
                panel
                    .action_state()
                    .refusal
                    .clone()
                    .map(|refusal| (refusal.failure.message, refusal.failure.detail))
            })
        })
    }

    fn wait_for_run(&mut self) -> Vec<String> {
        let seen = self.helm.seen.clone();
        wait_until(&mut self.vcx, "ran helm", |_| seen.exists());
        let panel = self.panel.clone();
        wait_until(&mut self.vcx, "finished the run", |vcx| {
            vcx.update(|_, cx| panel.update(cx, |panel, _| panel.action_state().running.is_none()))
        });
        normalized(self.helm.args().expect("helm ran"))
    }
}

/// The spec's "Rollback without the mouse": History by its key, Down to
/// revision 1, Rollback's key, then Enter - a recoverable confirmation.
#[gpui_kit::test]
async fn rollback_by_keys_goes_to_historys_selection(cx: &mut TestAppContext) {
    let mut h = open(cx, "s8-rollback", "exit 0");
    h.press("2");
    h.press("down");
    h.press("down");
    assert_eq!(h.rollback_target(), Some(1));
    h.press("ctrl-r");
    h.wait_for_dialog();
    h.press("enter");
    assert_eq!(h.wait_for_run(), expected("rollback", &["web", "1"]));
    assert_eq!(h.refusal(), None, "it succeeded");
}

/// With the current revision selected - the default - Rollback goes to the
/// one before it; Escape cancels and runs nothing.
#[gpui_kit::test]
async fn escape_cancels_a_rollback(cx: &mut TestAppContext) {
    let mut h = open(cx, "s8-rollback-escape", "exit 0");
    assert_eq!(h.rollback_target(), Some(2), "the one before the current");
    h.press("ctrl-r");
    h.wait_for_dialog();
    h.press("escape");
    assert!(!h.dialog_open());
    assert_eq!(h.helm.args(), None, "nothing ran");
}

/// `helm` failing is shown on the panel with its own message.
#[gpui_kit::test]
async fn a_failed_rollback_shows_helms_message(cx: &mut TestAppContext) {
    let mut h = open(
        cx,
        "s8-rollback-fails",
        "echo 'Error: pre-rollback hook failed' >&2\nexit 1",
    );
    h.press("ctrl-r");
    h.wait_for_dialog();
    h.press("enter");
    h.wait_for_run();
    assert_eq!(
        h.refusal(),
        Some((
            "helm exited with status 1".into(),
            "Error: pre-rollback hook failed".into()
        ))
    );
}

/// Without a usable `helm`, Rollback asks nothing and says why.
#[gpui_kit::test]
async fn without_helm_rollback_says_why(cx: &mut TestAppContext) {
    let mut h = open(cx, "s8-no-helm", "exit 0");
    h.vcx.update(|_, cx| {
        set_status(
            HelmStatus::Unusable("helm isn't on your login shell's PATH.".into()),
            cx,
        )
    });
    h.press("ctrl-r");
    assert!(!h.dialog_open(), "nothing to confirm");
    assert_eq!(
        h.refusal().map(|(message, _)| message),
        Some("helm isn't on your login shell's PATH.".into())
    );
    assert_eq!(h.helm.args(), None);
}

/// The spec's "Enter cancels": Uninstall's confirmation opens on Cancel, so
/// Enter runs nothing; its deliberate shortcut uninstalls.
#[gpui_kit::test]
async fn uninstall_confirms_only_by_its_shortcut(cx: &mut TestAppContext) {
    let mut h = open(cx, "s8-uninstall-detail", "exit 0");
    h.press("ctrl-d");
    h.wait_for_dialog();
    h.press("enter");
    assert!(!h.dialog_open(), "Enter closed it");
    assert_eq!(h.helm.args(), None, "and ran nothing");

    h.press("ctrl-d");
    h.wait_for_dialog();
    h.press("secondary-backspace");
    assert_eq!(h.wait_for_run(), expected("uninstall", &["web"]));
}

/// While an action runs the panel says so, and a second action on the
/// release neither asks nor starts - the running line is the reason.
#[gpui_kit::test]
async fn a_running_action_refuses_a_second(cx: &mut TestAppContext) {
    let mut h = open(cx, "s8-running", "sleep 5");
    h.press("ctrl-r");
    h.wait_for_dialog();
    h.press("enter");
    let panel = h.panel.clone();
    wait_until(&mut h.vcx, "showed it running", |vcx| {
        vcx.update(|_, cx| panel.update(cx, |panel, _| panel.action_state().running.clone()))
            .is_some()
    });
    let running = h
        .vcx
        .update(|_, cx| panel.update(cx, |panel, _| panel.action_state().running.clone()));
    assert_eq!(running.as_deref(), Some("Rolling back web to revision 2"));

    h.press("ctrl-d");
    assert!(!h.dialog_open(), "no second question");
    let still = h
        .vcx
        .update(|_, cx| panel.update(cx, |panel, _| panel.action_state().running.clone()));
    assert_eq!(still, running, "the rollback is still what runs");
    assert_eq!(
        h.wait_for_run(),
        expected("rollback", &["web", "2"]),
        "and only it ran"
    );
    assert_eq!(h.refusal(), None, "nothing stale left behind");
}
