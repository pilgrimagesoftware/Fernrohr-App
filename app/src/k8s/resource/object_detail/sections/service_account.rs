//! A ServiceAccount's section: the Secrets it mounts and pulls images with,
//! as references.

use super::super::model::{ObjectField, ObjectSection};
use crate::k8s::object_ref::ObjectRef;
use k8s_openapi::api::core::v1::ServiceAccount;

pub(super) fn sections(account: &ServiceAccount, namespace: &str) -> Vec<ObjectSection> {
    let mut fields = Vec::new();
    let secret = |name: &str| ObjectRef::core("Secret", namespace, name);

    let secrets: Vec<ObjectRef> = account
        .secrets
        .iter()
        .flatten()
        .filter_map(|reference| reference.name.as_deref())
        .filter(|name| !name.is_empty())
        .map(secret)
        .collect();
    if !secrets.is_empty() {
        fields.push(ObjectField::references("Secrets", secrets, false));
    }
    let pull_secrets: Vec<ObjectRef> = account
        .image_pull_secrets
        .iter()
        .flatten()
        .map(|reference| reference.name.as_str())
        .filter(|name| !name.is_empty())
        .map(secret)
        .collect();
    if !pull_secrets.is_empty() {
        fields.push(ObjectField::references(
            "Image Pull Secrets",
            pull_secrets,
            false,
        ));
    }
    if let Some(automount) = account.automount_service_account_token {
        fields.push(ObjectField::text(
            "Automount Token",
            if automount { "Yes" } else { "No" },
        ));
    }

    vec![ObjectSection::new("Service Account", fields)]
}
