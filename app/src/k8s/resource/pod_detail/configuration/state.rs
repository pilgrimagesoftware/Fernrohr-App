//! What the Configuration tab knows: each card's contents, and the Secret
//! values currently revealed.

use crate::k8s::object_ref::ObjectRef;
pub(in crate::k8s::resource::pod_detail) use crate::k8s::resource::secret_value::Reveal;
use std::collections::HashMap;

/// One card's contents, as far as they've loaded. A Secret's are its type and
/// keys with sizes only - its values are dropped as the object arrives.
#[derive(Debug)]
pub(in crate::k8s::resource::pod_detail) enum CardContents {
    Loading,
    ConfigMap {
        data: Vec<(String, String)>,
        binary: Vec<(String, usize)>,
    },
    Secret {
        type_: Option<String>,
        keys: Vec<(String, usize)>,
    },
    NotFound,
    Failed(String),
}

/// The tab's state on one panel. `Debug` prints revealed values through
/// `SecretValue`'s own `Debug`, which is the size, never the value.
#[derive(Debug, Default)]
pub(in crate::k8s::resource::pod_detail) struct ConfigurationState {
    /// Whether the cards' objects have been requested - once per panel.
    pub(in crate::k8s::resource::pod_detail) requested: bool,
    pub(in crate::k8s::resource::pod_detail) cards: HashMap<ObjectRef, CardContents>,
    /// Revealed (or revealing) values, keyed by Secret and key.
    pub(in crate::k8s::resource::pod_detail) revealed: HashMap<(ObjectRef, String), Reveal>,
}

impl ConfigurationState {
    /// Hides every revealed value: the `SecretValue`s are dropped, not kept
    /// behind a flag.
    pub(in crate::k8s::resource::pod_detail) fn hide_all(&mut self) {
        self.revealed.clear();
    }

    pub(in crate::k8s::resource::pod_detail) fn reveal_of(
        &self,
        secret: &ObjectRef,
        key: &str,
    ) -> Option<&Reveal> {
        self.revealed.get(&(secret.clone(), key.to_string()))
    }
}
