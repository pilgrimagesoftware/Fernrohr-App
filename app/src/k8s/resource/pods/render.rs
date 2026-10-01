//! Drawing the Pods panel: its table, hint bar and states.

use super::*;

impl Render for PodsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::k8s::cluster::connection::ConnectionState;

        let content = match &self.connection.read(cx).state {
            ConnectionState::Connecting => div().size_full().p_3().child("Connecting..."),
            ConnectionState::WaitingForTunnel => {
                div().size_full().p_3().child("Waiting for tunnel...")
            }
            ConnectionState::Failed(reason) => div()
                .size_full()
                .p_3()
                .child(format!("Connection failed: {reason}")),
            // `connection-status-bar`: a paused watch used to print its own "Paused
            // (...)" line here. That moved to the window's status bar
            // (`ui/status_bar.rs`), which shows it once per window rather than once per
            // affected panel - this branch no longer reads pause state at all, so a
            // paused panel just keeps rendering its last rows undisturbed.
            ConnectionState::Connected(_) => {
                let now = Timestamp::now();
                let namespaces = &self.scope.namespaces;
                let items: Vec<PodTableRow> = self
                    .table
                    .read(cx)
                    .pods()
                    .iter()
                    .filter(|pod| matches_namespaces(pod, namespaces))
                    .map(|pod| {
                        let containers = pod
                            .spec
                            .as_ref()
                            .map(|spec| spec.containers.iter().map(|c| c.name.clone()).collect())
                            .unwrap_or_default();
                        let selection = PodSelection {
                            namespace: pod.metadata.namespace.clone().unwrap_or_default(),
                            name: pod.metadata.name.clone().unwrap_or_default(),
                            containers,
                            context_name: self.scope.context_name.clone(),
                        };
                        PodTableRow {
                            row: pod_row(pod, now),
                            selection,
                        }
                    })
                    .collect();
                let table = self.sync_table(items, window, cx);
                let namespace_key =
                    Kbd::binding_for_action(&WarpNamespace, Some(PANEL_KEY_CONTEXT), window)
                        .unwrap_or_else(|| {
                            Kbd::new(Keystroke::parse(NAMESPACE_KEY).expect("valid keybinding"))
                        });
                let describe_key =
                    Kbd::binding_for_action(&DescribePod, Some(PANEL_KEY_CONTEXT), window)
                        .unwrap_or_else(|| {
                            Kbd::new(Keystroke::parse(DESCRIBE_KEY).expect("valid keybinding"))
                        });
                let logs_key =
                    Kbd::binding_for_action(&ShowPodLogs, Some(PANEL_KEY_CONTEXT), window)
                        .unwrap_or_else(|| {
                            Kbd::new(Keystroke::parse(LOGS_KEY).expect("valid keybinding"))
                        });
                let yaml_key =
                    Kbd::binding_for_action(&ShowPodYaml, Some(PANEL_KEY_CONTEXT), window)
                        .unwrap_or_else(|| {
                            Kbd::new(Keystroke::parse(YAML_KEY).expect("valid keybinding"))
                        });
                let shortcuts = div()
                    .flex()
                    .gap_3()
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .items_center()
                            .child(namespace_key)
                            .child("Namespace"),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .items_center()
                            .child(describe_key)
                            .child("Describe"),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .items_center()
                            .child(logs_key)
                            .child("Logs"),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_1()
                            .items_center()
                            .child(yaml_key)
                            .child("YAML"),
                    );
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .p_3()
                    .child(
                        div().flex_1().min_h_0().child(
                            DataTable::new(&table)
                                .stripe(true)
                                .bordered(true)
                                .scrollbar_visible(true, true),
                        ),
                    )
                    .child(
                        div()
                            .mt_2()
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .bg(crate::ui::style::surface_raised(cx))
                            .child(shortcuts),
                    )
            }
        };

        let this = cx.weak_entity();
        let namespaces = self.namespaces.read(cx).names();
        let namespace_bar =
            panel_title::namespace_picker(&self.scope, namespaces, move |namespaces, cx| {
                let _ = this.update(cx, |this: &mut Self, cx| {
                    this.scope = this.scope.scoped_to(namespaces.clone());
                    cx.emit(ScopeEvent::NamespacesChanged(namespaces));
                });
            });
        // "Context: <name>" on the left, the namespace picker (when there is one)
        // on the right, in one header row.
        let header = div()
            .flex()
            .items_center()
            .gap_2()
            .p_2()
            .bg(crate::ui::style::surface_raised(cx))
            .border_b_1()
            .border_color(cx.theme().border)
            .child(panel_title::context_label(
                &self.scope,
                cx.theme().muted_foreground,
            ))
            .children(namespace_bar);

        div()
            .size_full()
            .key_context(PANEL_KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_action_warp_namespace))
            .on_action(cx.listener(Self::on_action_describe_pod))
            .on_action(cx.listener(Self::on_action_show_pod_logs))
            .on_action(cx.listener(Self::on_action_show_pod_yaml))
            .child(
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .child(header)
                    .child(div().flex_1().min_h_0().child(content)),
            )
    }
}
