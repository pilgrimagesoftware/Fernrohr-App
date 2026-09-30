//! The fetch against a fixture API server: found, events forbidden, and 404.

use crate::k8s::resource::pod_detail::fetch::{PodFetch, fetch_pod};

/// The fetch tells a 404 apart from every other failure, against a real
/// (fixture) client - one that serves a pod, and 404s for anything else.
#[test]
fn fetch_distinguishes_a_missing_pod_from_other_failures() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let (addr, _server) = runtime.block_on(spawn_fixture_api_server());

    runtime.block_on(async {
        let client =
            kube::Client::try_from(kube::Config::new(format!("http://{addr}").parse().unwrap()))
                .expect("build client");

        let found = fetch_pod(client.clone(), "staging".into(), "present".into())
            .await
            .expect("a 200 is not a failure");
        match found {
            PodFetch::Found(pod, events) => {
                assert_eq!(pod.metadata.name.as_deref(), Some("present"));
                let events = events.expect("the server lists this pod's events");
                assert_eq!(events.len(), 1);
                assert_eq!(events[0].reason.as_deref(), Some("Scheduled"));
            }
            PodFetch::NotFound => panic!("the server serves this pod"),
        }

        // A pod readable by a user who may not list events still loads;
        // the events failure is carried alongside it, not swallowed.
        let forbidden = fetch_pod(client.clone(), "staging".into(), "events-forbidden".into())
            .await
            .expect("an events failure does not fail the pod");
        match forbidden {
            PodFetch::Found(pod, events) => {
                assert_eq!(pod.metadata.name.as_deref(), Some("events-forbidden"));
                assert!(events.is_err(), "the 403 is reported, not an empty list");
            }
            PodFetch::NotFound => panic!("the server serves this pod"),
        }

        let missing = fetch_pod(client, "staging".into(), "gone".into())
            .await
            .expect("a 404 is a state, not a failure");
        assert!(matches!(missing, PodFetch::NotFound));
    });
}

/// A fixture API server serving one pod by name and 404ing every other
/// name - the smallest thing that exercises `fetch_pod`'s error split.
async fn spawn_fixture_api_server() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buffer = vec![0u8; 4096];
                let read = stream.read(&mut buffer).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..read]).to_string();
                // Only the request line: a header could name the pod too.
                let request_line = request.lines().next().unwrap_or_default();
                let (status, body) = if request_line.contains("/events?") {
                    if request_line.contains("events-forbidden") {
                        let status = serde_json::json!({
                            "kind": "Status",
                            "apiVersion": "v1",
                            "status": "Failure",
                            "message": "events is forbidden",
                            "reason": "Forbidden",
                            "code": 403,
                        });
                        ("403 Forbidden", status.to_string())
                    } else {
                        let events = serde_json::json!({
                            "apiVersion": "v1",
                            "kind": "EventList",
                            "metadata": {},
                            "items": [{
                                "metadata": { "name": "present.1", "namespace": "staging" },
                                "involvedObject": { "kind": "Pod", "name": "present" },
                                "reason": "Scheduled",
                                "type": "Normal",
                            }],
                        });
                        ("200 OK", events.to_string())
                    }
                } else if let Some(name) = ["present", "events-forbidden"]
                    .into_iter()
                    .find(|name| request_line.contains(&format!("/pods/{name} ")))
                {
                    let pod = serde_json::json!({
                        "apiVersion": "v1",
                        "kind": "Pod",
                        "metadata": { "name": name, "namespace": "staging" },
                    });
                    ("200 OK", pod.to_string())
                } else {
                    let status = serde_json::json!({
                        "kind": "Status",
                        "apiVersion": "v1",
                        "status": "Failure",
                        "message": "pods \"gone\" not found",
                        "reason": "NotFound",
                        "code": 404,
                    });
                    ("404 Not Found", status.to_string())
                };
                let response = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.flush().await;
            });
        }
    });
    (addr, handle)
}
