use crate::k8s::cluster::connection::{ClusterConnection, ConnectionState};
use crate::k8s::cluster::session::ClusterRegistry;
use gpui_kit::*;
use k8s_openapi::api::core::v1::Namespace;
use kube::{Api, api::ListParams};
use std::collections::HashMap;

#[derive(Default)]
pub struct NamespaceRegistry(HashMap<String, Entity<NamespaceList>>);

impl Global for NamespaceRegistry {}

impl NamespaceRegistry {
    pub fn list(cx: &mut App, context_name: &str) -> Entity<NamespaceList> {
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
        if let Some(list) = cx.global::<Self>().0.get(context_name) {
            return list.clone();
        }
        let context_name = context_name.to_string();
        let list = cx.new(|cx| NamespaceList::new(context_name.clone(), cx));
        cx.global_mut::<Self>().0.insert(context_name, list.clone());
        list
    }
}

impl NamespaceRegistry {
    /// Makes `list` `context_name`'s namespace list - a test's cluster.
    #[cfg(test)]
    pub(crate) fn set_for_test(cx: &mut App, context_name: &str, list: Entity<NamespaceList>) {
        cx.default_global::<Self>()
            .0
            .insert(context_name.to_string(), list);
    }
}

pub struct NamespaceList {
    names: Vec<String>,
    loading: bool,
    loaded: bool,
}

impl NamespaceList {
    /// A list that never syncs: no connection is observed, so nothing is
    /// fetched. For tests that only need the entity to exist.
    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self {
            names: Vec::new(),
            loading: false,
            loaded: false,
        }
    }

    /// A list holding `names`, never syncing - a test's cluster.
    #[cfg(test)]
    pub(crate) fn with_names(names: &[&str]) -> Self {
        Self {
            names: names.iter().map(|name| name.to_string()).collect(),
            loading: false,
            loaded: true,
        }
    }

    fn new(context_name: String, cx: &mut Context<Self>) -> Self {
        let connection = ClusterRegistry::connection(cx, &context_name);
        cx.observe(&connection, |this: &mut Self, connection, cx| {
            this.sync(&connection, cx);
        })
        .detach();
        let mut this = Self {
            names: Vec::new(),
            loading: false,
            loaded: false,
        };
        this.sync(&connection, cx);
        this
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    fn sync(&mut self, connection: &Entity<ClusterConnection>, cx: &mut Context<Self>) {
        if self.loading || self.loaded {
            return;
        }
        let ConnectionState::Connected(client) = &connection.read(cx).state else {
            return;
        };
        self.loading = true;
        let client = client.clone();
        let rx = crate::runtime::spawn_stream(cx, 1, move |tx| async move {
            let _ = tx.send(fetch_names(client).await).await;
        });
        cx.spawn(async move |this, cx| {
            crate::runtime::drain(rx, |result| {
                let _ = this.update(cx, |this, cx| {
                    this.loading = false;
                    this.loaded = true;
                    this.names = result.unwrap_or_default();
                    cx.notify();
                });
            })
            .await;
        })
        .detach();
    }
}

async fn fetch_names(client: kube::Client) -> Result<Vec<String>, kube::Error> {
    let namespaces = Api::<Namespace>::all(client)
        .list(&ListParams::default())
        .await?;
    let mut names: Vec<String> = namespaces
        .items
        .into_iter()
        .filter_map(|namespace| namespace.metadata.name)
        .collect();
    names.sort_unstable();
    names.dedup();
    Ok(names)
}

#[cfg(test)]
mod tests {
    #[test]
    fn namespace_names_are_sorted_and_unique() {
        let mut names = vec![
            "kube-system".to_string(),
            "default".to_string(),
            "default".to_string(),
        ];
        names.sort_unstable();
        names.dedup();
        assert_eq!(names, ["default", "kube-system"]);
    }
}
