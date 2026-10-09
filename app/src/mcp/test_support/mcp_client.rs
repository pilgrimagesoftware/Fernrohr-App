//! A scripted MCP client: newline-delimited JSON-RPC over an in-memory pipe,
//! in place of an agent's stdio, to a real `fernrohr mcp` adapter
//! (`adapter::serve`, the same code `fernrohr mcp` runs on stdio) forwarding
//! to the endpoint at the paths it is given.

use crate::mcp::adapter::serve;
use crate::mcp::client::AppClient;
use crate::mcp::endpoint::EndpointPaths;
use serde_json::{Value, json};
use tokio::io::{
    AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf,
};

/// A scripted MCP client talking to one adapter.
pub(in crate::mcp) struct McpClient {
    lines: Lines<BufReader<ReadHalf<DuplexStream>>>,
    writer: WriteHalf<DuplexStream>,
    next_id: u64,
}

impl McpClient {
    /// Starts an adapter forwarding to the endpoint at `paths`, and completes
    /// MCP initialization with it.
    pub(in crate::mcp) async fn connect(paths: EndpointPaths) -> Self {
        let (ours, theirs) = tokio::io::duplex(1 << 20);
        tokio::spawn(serve(AppClient::new(paths), tokio::io::split(theirs)));
        let (reader, writer) = tokio::io::split(ours);
        let mut client = Self {
            lines: BufReader::new(reader).lines(),
            writer,
            next_id: 0,
        };
        let init = client
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {"name": "fixture", "version": "0"},
                }),
            )
            .await;
        assert_eq!(init["result"]["serverInfo"]["name"], "fernrohr");
        assert!(init["result"]["capabilities"]["tools"].is_object());
        client
            .send(json!({"jsonrpc": "2.0", "method": "notifications/initialized"}))
            .await;
        client
    }

    async fn send(&mut self, message: Value) {
        let mut line = message.to_string();
        line.push('\n');
        self.writer.write_all(line.as_bytes()).await.unwrap();
    }

    /// Sends a request and returns its response, skipping any notification
    /// the server sends in between.
    pub(in crate::mcp) async fn request(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}))
            .await;
        loop {
            let line =
                tokio::time::timeout(std::time::Duration::from_secs(10), self.lines.next_line())
                    .await
                    .expect("the adapter answers")
                    .unwrap()
                    .expect("the adapter keeps the stream open");
            let message: Value = serde_json::from_str(&line).unwrap();
            if message["id"] == id {
                return message;
            }
        }
    }
}
