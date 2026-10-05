//! The bridge against in-memory pipes standing in for an attached exec: the
//! container's output arrives as events, the user's input reaches its stdin,
//! and the bridge finishes when the container's streams end.

use super::{ExecEvent, bridge};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::sync::mpsc;

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn output_flows_out_input_flows_in_and_the_end_ends_it() {
    runtime().block_on(async {
        let (mut container_out, app_out) = tokio::io::duplex(1024);
        let (mut container_err, app_err) = tokio::io::duplex(1024);
        let (app_in, mut container_in) = tokio::io::duplex(1024);
        let (input_tx, input_rx) = mpsc::channel(8);
        let (events_tx, mut events_rx) = mpsc::channel(8);
        let pump = tokio::spawn(bridge(
            Some(app_out),
            Some(app_err),
            Some(app_in),
            input_rx,
            events_tx,
        ));

        container_out.write_all(b"hello\n").await.unwrap();
        assert_eq!(
            events_rx.recv().await,
            Some(ExecEvent::Output("hello\n".into()))
        );
        container_err.write_all(b"oops\n").await.unwrap();
        assert_eq!(
            events_rx.recv().await,
            Some(ExecEvent::Output("oops\n".into()))
        );

        input_tx.send(b"ls\n".to_vec()).await.unwrap();
        let mut typed = [0u8; 3];
        container_in.read_exact(&mut typed).await.unwrap();
        assert_eq!(&typed, b"ls\n", "the line reached the container's stdin");

        drop(container_out);
        drop(container_err);
        tokio::time::timeout(std::time::Duration::from_secs(5), pump)
            .await
            .expect("the bridge finishes when the container's streams end")
            .unwrap();
    });
}

#[test]
fn closing_the_input_closes_the_containers_stdin() {
    runtime().block_on(async {
        let (_container_out, app_out) = tokio::io::duplex(64);
        let (app_in, mut container_in) = tokio::io::duplex(64);
        let (input_tx, input_rx) = mpsc::channel(8);
        let (events_tx, _events_rx) = mpsc::channel(8);
        let _pump = tokio::spawn(bridge(
            Some(app_out),
            None::<tokio::io::DuplexStream>,
            Some(app_in),
            input_rx,
            events_tx,
        ));

        drop(input_tx);
        let mut rest = Vec::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(5),
            container_in.read_to_end(&mut rest),
        )
        .await
        .expect("stdin closes once the panel's input does")
        .unwrap();
        assert!(rest.is_empty());
    });
}
