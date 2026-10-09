//! The tool surface: what each tool is ([`ToolSpec`]), what it returns
//! ([`ToolOutput`]), what it may use ([`ToolContext`]), and the
//! [`ToolRegistry`] the endpoint serves them from.
//!
//! Tools live only in the app. The adapter has no tool list of its own: it
//! asks the app for [`ToolRegistry::specs`] and forwards each call by name, so
//! adding a tool is one [`ToolRegistry::register`] call in [`ToolRegistry::app`]
//! and nothing on the adapter side.
//!
//! A handler takes its own typed input. `register` deserializes the client's
//! arguments into it before the handler runs and answers a mismatch with
//! [`ToolError::InvalidArguments`], so no handler ever sees raw JSON.

use super::error::ToolError;
use super::foreground::Foreground;
use futures_util::FutureExt;
use futures_util::future::BoxFuture;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;

/// What a tool does, which decides how the endpoint treats a call to it:
/// actions alone go through the user's confirmation (section 3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ToolKind {
    /// Reads cluster state; changes nothing.
    Read,
    /// Opens or focuses something in the app's own UI; changes no cluster.
    Navigate,
    /// One of the allowlisted cluster actions.
    Action,
}

/// One tool as an MCP client lists it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct ToolSpec {
    pub(super) name: String,
    pub(super) title: String,
    pub(super) description: String,
    pub(super) kind: ToolKind,
    /// The JSON Schema of the tool's arguments: always an `object` schema.
    pub(super) input_schema: Map<String, Value>,
}

/// A tool's successful result: the structured object the client receives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct ToolOutput {
    pub(super) content: Map<String, Value>,
    /// The tool cut its result short to stay under the reply limit; the
    /// client is told so it can ask for less (section 2.3).
    #[serde(default)]
    pub(super) truncated: bool,
}

impl ToolOutput {
    // UNWIRED(#189): tool handlers (sections 2-4) build their results with it.
    #[allow(dead_code)]
    pub(super) fn new(content: Map<String, Value>) -> Self {
        Self {
            content,
            truncated: false,
        }
    }
}

/// What a running handler may use. Cloned into every call.
#[derive(Clone)]
pub(super) struct ToolContext {
    /// The main thread, for any GPUI state the tool reads or changes.
    // UNWIRED(#189): read by the tool handlers of sections 2-4.
    #[allow(dead_code)]
    pub(super) foreground: Foreground,
}

type Handler =
    Arc<dyn Fn(Map<String, Value>, ToolContext) -> BoxFuture<'static, ToolResult> + Send + Sync>;

/// A call's result as it crosses the endpoint.
pub(super) type ToolResult = Result<ToolOutput, ToolError>;

struct Registered {
    spec: ToolSpec,
    handler: Handler,
}

/// Every tool the endpoint serves, by name.
#[derive(Default)]
pub(super) struct ToolRegistry {
    tools: BTreeMap<String, Registered>,
}

impl ToolRegistry {
    /// The app's tools. Empty until the cluster (section 2), action
    /// (section 3) and navigation (section 4) tools register here.
    pub(super) fn app() -> Self {
        Self::default()
    }

    /// Adds a tool whose handler takes the typed input `A`.
    ///
    /// # Panics
    ///
    /// If a tool of the same name is already registered: tool names are fixed
    /// in code, so a clash is a programming error, caught by any test that
    /// builds the registry.
    // UNWIRED(#189): sections 2-4 register the app's tools in `app`.
    #[allow(dead_code)]
    pub(super) fn register<A, F, Fut>(&mut self, spec: ToolSpec, handler: F)
    where
        A: DeserializeOwned,
        F: Fn(A, ToolContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ToolResult> + Send + 'static,
    {
        let handler = Arc::new(handler);
        let erased: Handler = Arc::new(move |arguments, cx| {
            match serde_json::from_value::<A>(Value::Object(arguments)) {
                Ok(input) => handler(input, cx).boxed(),
                Err(error) => std::future::ready(Err(ToolError::invalid_arguments(&error))).boxed(),
            }
        });
        let name = spec.name.clone();
        let previous = self.tools.insert(
            name.clone(),
            Registered {
                spec,
                handler: erased,
            },
        );
        assert!(previous.is_none(), "tool {name:?} is registered twice");
    }

    /// Every tool's spec, in name order.
    pub(super) fn specs(&self) -> Vec<ToolSpec> {
        self.tools
            .values()
            .map(|registered| registered.spec.clone())
            .collect()
    }

    /// Runs the tool `name` with `arguments`. The returned future owns
    /// everything it needs, so the caller may drop it to cancel the call.
    pub(super) fn call(
        &self,
        name: &str,
        arguments: Map<String, Value>,
        cx: ToolContext,
    ) -> BoxFuture<'static, ToolResult> {
        match self.tools.get(name) {
            Some(registered) => (registered.handler)(arguments, cx),
            None => std::future::ready(Err(ToolError::UnknownTool {
                name: name.to_string(),
            }))
            .boxed(),
        }
    }
}

#[cfg(test)]
mod tests {
    //! `register` has no production caller until section 2; these pin the
    //! contract the app's tools will be added under.

    use super::*;
    use crate::mcp::test_support::{echo_spec, echo_tool_registry, object, test_context};
    use serde_json::json;

    #[tokio::test]
    async fn a_call_reaches_its_handler_with_typed_input() {
        let registry = echo_tool_registry();
        let output = registry
            .call("echo", object(json!({"text": "hi"})), test_context())
            .await
            .unwrap();
        assert_eq!(output.content, object(json!({"echo": "hi"})));
    }

    #[tokio::test]
    async fn arguments_that_dont_fit_the_input_never_reach_the_handler() {
        let registry = echo_tool_registry();
        let result = registry
            .call("echo", object(json!({"text": 3})), test_context())
            .await;
        assert!(matches!(result, Err(ToolError::InvalidArguments { .. })));
    }

    #[tokio::test]
    async fn an_unknown_tool_is_named_in_its_error() {
        let result = ToolRegistry::app()
            .call("nope", Map::new(), test_context())
            .await;
        assert_eq!(
            result,
            Err(ToolError::UnknownTool {
                name: "nope".into()
            })
        );
    }

    #[test]
    fn specs_list_every_tool_in_name_order() {
        let mut registry = echo_tool_registry();
        let mut first = echo_spec();
        first.name = "a_first".into();
        registry.register(first, |_: Value, _| async {
            Ok(ToolOutput::new(Map::new()))
        });
        let names: Vec<_> = registry.specs().into_iter().map(|s| s.name).collect();
        assert_eq!(names, ["a_first", "echo"]);
    }

    #[test]
    #[should_panic(expected = "registered twice")]
    fn a_name_cannot_be_registered_twice() {
        let mut registry = echo_tool_registry();
        registry.register(echo_spec(), |_: Value, _| async {
            Ok(ToolOutput::new(Map::new()))
        });
    }

    #[test]
    fn the_app_has_no_state_changing_tool_outside_the_allowlist() {
        // Section 3 fills in the allowlist; until then the app has no action
        // tool at all, and this is where adding one has to be acknowledged.
        let actions: Vec<_> = ToolRegistry::app()
            .specs()
            .into_iter()
            .filter(|spec| spec.kind == ToolKind::Action)
            .map(|spec| spec.name)
            .collect();
        assert!(actions.is_empty(), "unexpected action tools: {actions:?}");
    }
}
