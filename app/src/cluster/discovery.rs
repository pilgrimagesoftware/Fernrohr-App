use kube::Client;
use kube::discovery::Discovery;
use std::collections::BTreeSet;

/// Runs API discovery against `client` and returns the distinct resource
/// kind names found across every discovered api group (core and otherwise).
pub async fn discover_kinds(client: Client) -> kube::Result<Vec<String>> {
    let discovery = Discovery::new(client).run().await?;
    let mut kinds: BTreeSet<String> = BTreeSet::new();
    for group in discovery.groups() {
        for (resource, _capabilities) in group.recommended_resources() {
            kinds.insert(resource.kind);
        }
    }
    Ok(kinds.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kube::{Client, Config};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    /// Serves fixed JSON responses by request path until the listener (and
    /// every clone of it) is dropped, i.e. for the lifetime of the test.
    async fn serve_fixtures(routes: HashMap<&'static str, &'static str>) -> std::net::SocketAddr {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let routes = Arc::new(routes);
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    return;
                };
                let routes = routes.clone();
                tokio::spawn(handle_connection(stream, routes));
            }
        });
        addr
    }

    async fn handle_connection(
        stream: tokio::net::TcpStream,
        routes: Arc<HashMap<&'static str, &'static str>>,
    ) {
        let mut reader = BufReader::new(stream);
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).await.unwrap_or(0) == 0 {
            return;
        }
        // Drain headers up to the blank line.
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).await.unwrap_or(0) == 0 || line == "\r\n" {
                break;
            }
        }

        let path = request_line
            .split_whitespace()
            .nth(1)
            .unwrap_or("")
            .to_string();
        let stream = reader.into_inner();
        respond(stream, routes.get(path.as_str()).copied()).await;
    }

    async fn respond(mut stream: tokio::net::TcpStream, body: Option<&str>) {
        let (status, body) = match body {
            Some(body) => ("200 OK", body),
            None => ("404 Not Found", "{}"),
        };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
    }

    fn client_for(addr: std::net::SocketAddr) -> Client {
        let mut config = Config::new(format!("http://{addr}").parse().unwrap());
        config.connect_timeout = Some(std::time::Duration::from_millis(500));
        Client::try_from(config).unwrap()
    }

    #[tokio::test]
    async fn pod_is_present_after_discovery() {
        let routes = HashMap::from([
            (
                "/apis",
                r#"{"kind":"APIGroupList","apiVersion":"v1","groups":[]}"#,
            ),
            (
                "/api",
                r#"{"kind":"APIVersions","versions":["v1"],"serverAddressByClientCIDRs":[]}"#,
            ),
            (
                "/api/v1",
                r#"{"kind":"APIResourceList","groupVersion":"v1","resources":[
                    {"name":"pods","singularName":"pod","namespaced":true,"kind":"Pod","verbs":["get","list","watch"]}
                ]}"#,
            ),
        ]);
        let addr = serve_fixtures(routes).await;

        let kinds = discover_kinds(client_for(addr)).await.unwrap();

        assert!(kinds.contains(&"Pod".to_string()), "kinds: {kinds:?}");
    }
}
