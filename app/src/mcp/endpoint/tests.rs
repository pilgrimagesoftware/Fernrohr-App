use super::*;
use crate::mcp::test_support::{dir_of, temp_endpoint_paths};

fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[tokio::test]
async fn binding_creates_owner_only_files() {
    let paths = temp_endpoint_paths();
    let endpoint = Endpoint::bind(paths.clone()).unwrap();

    assert_eq!(mode(&dir_of(&paths)), 0o700);
    assert_eq!(mode(&paths.token), 0o600);
    assert_eq!(mode(&paths.socket), 0o600);
    let on_disk = EndpointToken::read(&paths.token).unwrap();
    assert!(on_disk.matches(&endpoint.files.token));
    assert_eq!(
        endpoint.owner_uid,
        fs::metadata(dir_of(&paths)).unwrap().uid()
    );
    endpoint.files.remove();
}

#[tokio::test]
async fn a_crashed_runs_files_are_replaced() {
    let paths = temp_endpoint_paths();
    create_private_dir(&dir_of(&paths)).unwrap();
    // A socket file with nothing listening, as a killed app leaves it.
    drop(std::os::unix::net::UnixListener::bind(&paths.socket).unwrap());
    fs::write(&paths.token, "stale-token").unwrap();

    let endpoint = Endpoint::bind(paths.clone()).unwrap();
    let on_disk = EndpointToken::read(&paths.token).unwrap();
    assert_ne!(on_disk.expose(), "stale-token");
    assert!(on_disk.matches(&endpoint.files.token));
    endpoint.files.remove();
}

#[tokio::test]
async fn a_live_endpoint_is_left_to_its_app() {
    let paths = temp_endpoint_paths();
    let first = Endpoint::bind(paths.clone()).unwrap();

    assert!(matches!(
        Endpoint::bind(paths.clone()),
        Err(BindError::InUse)
    ));
    let on_disk = EndpointToken::read(&paths.token).unwrap();
    assert!(
        on_disk.matches(&first.files.token),
        "the first app's token stays"
    );
    first.files.remove();
}

#[tokio::test]
async fn each_launch_gets_a_new_token() {
    let paths = temp_endpoint_paths();
    let first = Endpoint::bind(paths.clone()).unwrap();
    let first_token = first.files.token.clone();
    first.files.remove();
    drop(first);

    let second = Endpoint::bind(paths.clone()).unwrap();
    assert!(!second.files.token.matches(&first_token));
    second.files.remove();
}

#[tokio::test]
async fn quitting_removes_only_this_runs_files() {
    let paths = temp_endpoint_paths();
    let endpoint = Endpoint::bind(paths.clone()).unwrap();
    endpoint.files.remove();
    assert!(!paths.socket.exists());
    assert!(!paths.token.exists());

    // A later run's files survive an older run's quit.
    let older = endpoint.files.clone();
    drop(endpoint);
    let later = Endpoint::bind(paths.clone()).unwrap();
    older.remove();
    assert!(paths.socket.exists());
    assert!(paths.token.exists());
    later.files.remove();
}

#[test]
fn a_file_in_the_directorys_place_is_refused() {
    let paths = temp_endpoint_paths();
    let dir = dir_of(&paths);
    fs::write(&dir, "not a directory").unwrap();
    assert!(create_private_dir(&dir).is_err());
    fs::remove_file(&dir).unwrap();
}
