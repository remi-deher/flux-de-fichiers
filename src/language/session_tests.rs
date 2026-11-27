use super::*;
use crate::language::codec::{read_message, write_message};
use forgearr_shared::language::{DiagnosticSeverity, SuggestionKind};
use tokio::{
    io::{BufReader, DuplexStream, duplex},
    time::timeout,
};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

const PIPE_BYTES: usize = 64 * 1024;
const MAX_TEST_MESSAGE_BYTES: usize = 1 << 20;
const WAIT: Duration = Duration::from_secs(5);
const TEST_REQUEST_TIMEOUT: Duration = Duration::from_millis(100);

/// Le serveur de langage, joué par le test : il lit ce que la session lui écrit et lui répond à la main.
struct FakeServer {
    reads: BufReader<DuplexStream>,
    writes: Option<DuplexStream>,
    to_session: mpsc::Sender<ClientMessage>,
    from_session: mpsc::Receiver<ServerMessage>,
    task: JoinHandle<EndReason>,
    uri: String,
}

fn spec_with(timings: Timings) -> SessionSpec {
    SessionSpec {
        server: LanguageServer::Bash,
        id: Uuid::nil(),
        process_id: 7,
        timings,
    }
}

fn quick_timings() -> Timings {
    Timings {
        handshake: WAIT,
        request: TEST_REQUEST_TIMEOUT,
        idle: WAIT,
    }
}

fn start_with(spec: SessionSpec) -> FakeServer {
    let (session_writer, fake_reads) = duplex(PIPE_BYTES);
    let (fake_writes, session_reader) = duplex(PIPE_BYTES);
    let (to_session, from_client) = mpsc::channel(8);
    let (to_client, from_session) = mpsc::channel(8);
    let uri = protocol::document_uri(spec.id, spec.server);
    let task = tokio::spawn(run(
        spec,
        BufReader::new(session_reader),
        session_writer,
        from_client,
        to_client,
    ));
    FakeServer {
        reads: BufReader::new(fake_reads),
        writes: Some(fake_writes),
        to_session,
        from_session,
        task,
        uri,
    }
}

fn start() -> FakeServer {
    start_with(spec_with(quick_timings()))
}

impl FakeServer {
    /// Le prochain message que la session écrit au serveur.
    async fn heard(&mut self) -> TestResult<Value> {
        let reading = read_message(&mut self.reads, MAX_TEST_MESSAGE_BYTES);
        let message = timeout(WAIT, reading).await??;
        Ok(message.ok_or("la session a fermé l'entrée du serveur")?)
    }

    async fn says(&mut self, message: Value) -> TestResult {
        let writes = self.writes.as_mut().ok_or("le serveur s'est arrêté")?;
        write_message(writes, &message).await?;
        Ok(())
    }

    /// Le prochain message que la session rend à la page.
    async fn page_receives(&mut self) -> TestResult<ServerMessage> {
        let message = timeout(WAIT, self.from_session.recv()).await?;
        Ok(message.ok_or("la session n'a rien rendu à la page")?)
    }

    async fn page_sends(&mut self, message: ClientMessage) -> TestResult {
        self.to_session.send(message).await?;
        Ok(())
    }

    async fn answers_initialize(&mut self, capabilities: Value) -> TestResult {
        let initialize = self.heard().await?;
        assert_eq!(initialize["method"], protocol::INITIALIZE);
        let result = json!({"capabilities": capabilities});
        self.says(json!({"jsonrpc": "2.0", "id": initialize["id"], "result": result}))
            .await
    }

    /// Poignée de main complète jusqu'à `Ready`.
    async fn ready(&mut self) -> TestResult {
        self.answers_initialize(json!({})).await?;
        let initialized = self.heard().await?;
        assert_eq!(initialized["method"], protocol::INITIALIZED);
        assert_eq!(self.page_receives().await?, ServerMessage::Ready);
        Ok(())
    }

    /// La page ouvre le document ; le serveur entend le `didOpen`.
    async fn opened(&mut self, text: &str) -> TestResult<Value> {
        let text = text.to_owned();
        self.page_sends(ClientMessage::Open { text }).await?;
        self.heard().await
    }

    async fn finished(self) -> TestResult<EndReason> {
        Ok(timeout(WAIT, self.task).await??)
    }
}

fn position(line: u32, character: u32) -> Position {
    Position { line, character }
}

#[tokio::test]
async fn the_handshake_ends_with_ready_for_the_page() -> TestResult {
    let mut server = start();
    let initialize = server.heard().await?;
    assert_eq!(initialize["params"]["processId"], 7);
    let root = protocol::workspace_uri(Uuid::nil());
    assert_eq!(initialize["params"]["rootUri"], root);
    let id = initialize["id"].clone();
    server
        .says(json!({"jsonrpc": "2.0", "id": id, "result": {"capabilities": {}}}))
        .await?;
    let initialized = server.heard().await?;
    assert_eq!(initialized["method"], protocol::INITIALIZED);
    assert_eq!(server.page_receives().await?, ServerMessage::Ready);
    Ok(())
}

#[tokio::test]
async fn opening_sends_the_whole_document() -> TestResult {
    let mut server = start();
    server.ready().await?;
    let opened = server.opened("echo bonjour").await?;
    assert_eq!(opened["method"], protocol::DID_OPEN);
    assert_eq!(opened["params"]["textDocument"]["uri"], server.uri);
    assert_eq!(opened["params"]["textDocument"]["text"], "echo bonjour");
    Ok(())
}

#[tokio::test]
async fn a_second_open_and_a_change_before_opening_are_ignored() -> TestResult {
    let mut server = start();
    server.ready().await?;
    server
        .page_sends(ClientMessage::Change {
            version: 2,
            text: "avant".to_owned(),
        })
        .await?;
    let opened = server.opened("un").await?;
    assert_eq!(opened["params"]["textDocument"]["text"], "un");
    server
        .page_sends(ClientMessage::Open {
            text: "deux".to_owned(),
        })
        .await?;
    server
        .page_sends(ClientMessage::Change {
            version: 3,
            text: "trois".to_owned(),
        })
        .await?;
    let changed = server.heard().await?;
    assert_eq!(changed["method"], protocol::DID_CHANGE);
    assert_eq!(changed["params"]["contentChanges"][0]["text"], "trois");
    Ok(())
}

#[tokio::test]
async fn a_completion_reaches_the_server_and_its_answer_the_page() -> TestResult {
    let mut server = start();
    server.ready().await?;
    server.opened("ec").await?;
    let complete = ClientMessage::Complete {
        id: 11,
        position: position(0, 2),
    };
    server.page_sends(complete).await?;
    let asked = server.heard().await?;
    assert_eq!(asked["method"], protocol::COMPLETION);
    let expected = json!({"line": 0, "character": 2});
    assert_eq!(asked["params"]["position"], expected);
    let items = json!([{"label": "echo", "kind": 3}]);
    server
        .says(json!({"jsonrpc": "2.0", "id": asked["id"], "result": items}))
        .await?;
    let ServerMessage::Completion { id, items } = server.page_receives().await? else {
        return Err("une réponse de complétion était attendue".into());
    };
    assert_eq!(id, 11);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].label, "echo");
    assert_eq!(items[0].kind, SuggestionKind::Function);
    Ok(())
}

#[tokio::test]
async fn a_hover_reaches_the_server_and_its_answer_the_page() -> TestResult {
    let mut server = start();
    server.ready().await?;
    server.opened("echo").await?;
    server
        .page_sends(ClientMessage::Hover {
            id: 4,
            position: position(0, 1),
        })
        .await?;
    let asked = server.heard().await?;
    assert_eq!(asked["method"], protocol::HOVER);
    let result = json!({"contents": {"kind": "markdown", "value": "`echo`"}});
    server
        .says(json!({"jsonrpc": "2.0", "id": asked["id"], "result": result}))
        .await?;
    let expected = ServerMessage::Hover {
        id: 4,
        text: Some("`echo`".to_owned()),
    };
    assert_eq!(server.page_receives().await?, expected);
    Ok(())
}

#[tokio::test]
async fn a_request_before_the_document_is_open_is_answered_empty() -> TestResult {
    let mut server = start();
    server.ready().await?;
    let complete = ClientMessage::Complete {
        id: 1,
        position: position(0, 0),
    };
    server.page_sends(complete).await?;
    let expected = ServerMessage::Completion {
        id: 1,
        items: Vec::new(),
    };
    assert_eq!(server.page_receives().await?, expected);
    Ok(())
}

#[tokio::test]
async fn a_request_without_an_answer_is_answered_empty_after_the_delay() -> TestResult {
    let mut server = start();
    server.ready().await?;
    server.opened("echo").await?;
    let hover = ClientMessage::Hover {
        id: 9,
        position: position(0, 0),
    };
    server.page_sends(hover).await?;
    server.heard().await?;
    let expected = ServerMessage::Hover { id: 9, text: None };
    assert_eq!(server.page_receives().await?, expected);
    Ok(())
}

#[tokio::test]
async fn a_late_answer_is_ignored() -> TestResult {
    let mut server = start();
    server.ready().await?;
    server.opened("echo").await?;
    let complete = ClientMessage::Complete {
        id: 2,
        position: position(0, 0),
    };
    server.page_sends(complete).await?;
    let asked = server.heard().await?;
    let empty = ServerMessage::Completion {
        id: 2,
        items: Vec::new(),
    };
    assert_eq!(server.page_receives().await?, empty);
    let items = json!([{"label": "tard"}]);
    server
        .says(json!({"jsonrpc": "2.0", "id": asked["id"], "result": items}))
        .await?;
    let hover = ClientMessage::Hover {
        id: 3,
        position: position(0, 0),
    };
    server.page_sends(hover).await?;
    server.heard().await?;
    let expected = ServerMessage::Hover { id: 3, text: None };
    assert_eq!(server.page_receives().await?, expected);
    Ok(())
}

#[tokio::test]
async fn published_diagnostics_reach_the_page() -> TestResult {
    let mut server = start();
    server.ready().await?;
    server.opened("echo $x").await?;
    let range = json!({"start": {"line": 0, "character": 5}, "end": {"line": 0, "character": 7}});
    let diagnostic = json!({"range": range, "severity": 2, "message": "x non défini"});
    let params = json!({"uri": server.uri, "diagnostics": [diagnostic]});
    server
        .says(json!({"jsonrpc": "2.0", "method": protocol::PUBLISH_DIAGNOSTICS, "params": params}))
        .await?;
    let ServerMessage::Diagnostics { items } = server.page_receives().await? else {
        return Err("des diagnostics étaient attendus".into());
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].severity, DiagnosticSeverity::Warning);
    assert_eq!(items[0].range.start.character, 5);
    Ok(())
}

#[tokio::test]
async fn diagnostics_of_another_document_are_ignored() -> TestResult {
    let mut server = start();
    server.ready().await?;
    server.opened("echo").await?;
    let other = json!({"uri": "file:///ailleurs.sh", "diagnostics": []});
    server
        .says(json!({"jsonrpc": "2.0", "method": protocol::PUBLISH_DIAGNOSTICS, "params": other}))
        .await?;
    let hover = ClientMessage::Hover {
        id: 1,
        position: position(0, 0),
    };
    server.page_sends(hover).await?;
    server.heard().await?;
    let expected = ServerMessage::Hover { id: 1, text: None };
    assert_eq!(server.page_receives().await?, expected);
    Ok(())
}

#[tokio::test]
async fn the_servers_own_requests_are_answered() -> TestResult {
    let mut server = start();
    server.ready().await?;
    let params = json!({"items": [{"section": "a"}, {"section": "b"}]});
    let request = protocol::request(50, "workspace/configuration", params);
    server.says(request).await?;
    let answer = server.heard().await?;
    assert_eq!(answer["id"], 50);
    assert_eq!(answer["result"], json!([{}, {}]));
    Ok(())
}

#[tokio::test]
async fn a_server_request_during_the_handshake_does_not_block_it() -> TestResult {
    let mut server = start();
    let initialize = server.heard().await?;
    server
        .says(protocol::request(60, "client/registerCapability", json!({})))
        .await?;
    server
        .says(json!({"jsonrpc": "2.0", "id": initialize["id"], "result": {"capabilities": {}}}))
        .await?;
    let answer = server.heard().await?;
    assert_eq!(answer["id"], 60);
    let initialized = server.heard().await?;
    assert_eq!(initialized["method"], protocol::INITIALIZED);
    assert_eq!(server.page_receives().await?, ServerMessage::Ready);
    Ok(())
}

#[tokio::test]
async fn a_server_that_supports_pull_diagnostics_is_asked_after_each_change() -> TestResult {
    let mut server = start();
    let capabilities = json!({"diagnosticProvider": {}});
    server.answers_initialize(capabilities).await?;
    server.heard().await?;
    assert_eq!(server.page_receives().await?, ServerMessage::Ready);
    server.opened("x = 1").await?;
    let pull = server.heard().await?;
    assert_eq!(pull["method"], protocol::PULL_DIAGNOSTICS);
    let range = json!({"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 1}});
    let items = json!([{"range": range, "severity": 1, "message": "erreur"}]);
    let full = json!({"kind": "full", "items": items});
    server
        .says(json!({"jsonrpc": "2.0", "id": pull["id"], "result": full}))
        .await?;
    let ServerMessage::Diagnostics { items } = server.page_receives().await? else {
        return Err("des diagnostics étaient attendus".into());
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].severity, DiagnosticSeverity::Error);
    Ok(())
}

#[tokio::test]
async fn a_change_during_a_pull_asks_again_once_the_answer_arrives() -> TestResult {
    let mut server = start();
    let capabilities = json!({"diagnosticProvider": {}});
    server.answers_initialize(capabilities).await?;
    server.heard().await?;
    server.page_receives().await?;
    server.opened("a").await?;
    let first = server.heard().await?;
    assert_eq!(first["method"], protocol::PULL_DIAGNOSTICS);
    let change = ClientMessage::Change {
        version: 2,
        text: "ab".to_owned(),
    };
    server.page_sends(change).await?;
    let changed = server.heard().await?;
    assert_eq!(changed["method"], protocol::DID_CHANGE);
    let full = json!({"kind": "full", "items": []});
    server
        .says(json!({"jsonrpc": "2.0", "id": first["id"], "result": full}))
        .await?;
    assert_eq!(
        server.page_receives().await?,
        ServerMessage::Diagnostics { items: Vec::new() }
    );
    let second = server.heard().await?;
    assert_eq!(second["method"], protocol::PULL_DIAGNOSTICS);
    Ok(())
}

#[tokio::test]
async fn closing_asks_the_server_to_shut_down_and_ends_without_a_warning() -> TestResult {
    let mut server = start();
    server.ready().await?;
    server.page_sends(ClientMessage::Close).await?;
    let shutdown = server.heard().await?;
    assert_eq!(shutdown["method"], protocol::SHUTDOWN);
    let exit = server.heard().await?;
    assert_eq!(exit["method"], protocol::EXIT);
    let mut from_session = server.from_session;
    let reason = timeout(WAIT, server.task).await??;
    assert_eq!(reason, EndReason::ClientClosed);
    assert!(from_session.recv().await.is_none());
    Ok(())
}

#[tokio::test]
async fn a_page_that_disappears_ends_the_session() -> TestResult {
    let mut server = start();
    server.ready().await?;
    drop(server.to_session);
    let reason = timeout(WAIT, server.task).await??;
    assert_eq!(reason, EndReason::ClientClosed);
    Ok(())
}

#[tokio::test]
async fn silence_from_the_page_ends_the_session_and_warns_it() -> TestResult {
    let timings = Timings {
        idle: Duration::from_millis(150),
        ..quick_timings()
    };
    let mut server = start_with(spec_with(timings));
    server.ready().await?;
    let expected = ServerMessage::Unavailable {
        reason: UnavailableReason::ServerStopped,
    };
    assert_eq!(server.page_receives().await?, expected);
    assert_eq!(server.finished().await?, EndReason::Idle);
    Ok(())
}

#[tokio::test]
async fn a_message_from_the_page_restarts_the_idle_delay() -> TestResult {
    let timings = Timings {
        idle: Duration::from_millis(400),
        ..quick_timings()
    };
    let mut server = start_with(spec_with(timings));
    server.ready().await?;
    for _ in 0..3 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let hover = ClientMessage::Hover {
            id: 1,
            position: position(0, 0),
        };
        server.page_sends(hover).await?;
        let answered = server.page_receives().await?;
        assert_eq!(answered, ServerMessage::Hover { id: 1, text: None });
    }
    assert!(!server.task.is_finished());
    Ok(())
}

#[tokio::test]
async fn a_server_that_stops_ends_the_session_and_warns_the_page() -> TestResult {
    let mut server = start();
    server.ready().await?;
    server.writes = None;
    let expected = ServerMessage::Unavailable {
        reason: UnavailableReason::ServerStopped,
    };
    assert_eq!(server.page_receives().await?, expected);
    let reason = timeout(WAIT, server.task).await??;
    assert_eq!(reason, EndReason::ServerStopped);
    Ok(())
}

#[tokio::test]
async fn a_server_that_never_answers_the_handshake_is_given_up_on() -> TestResult {
    let timings = Timings {
        handshake: Duration::from_millis(150),
        ..quick_timings()
    };
    let mut server = start_with(spec_with(timings));
    server.heard().await?;
    let expected = ServerMessage::Unavailable {
        reason: UnavailableReason::ServerStopped,
    };
    assert_eq!(server.page_receives().await?, expected);
    assert_eq!(server.finished().await?, EndReason::ServerStopped);
    Ok(())
}

#[tokio::test]
async fn a_server_that_refuses_to_initialize_is_given_up_on() -> TestResult {
    let mut server = start();
    let initialize = server.heard().await?;
    let refusal = json!({"jsonrpc": "2.0", "id": initialize["id"], "error": {"code": -1}});
    server.says(refusal).await?;
    let expected = ServerMessage::Unavailable {
        reason: UnavailableReason::ServerStopped,
    };
    assert_eq!(server.page_receives().await?, expected);
    assert_eq!(server.finished().await?, EndReason::ServerStopped);
    Ok(())
}

#[tokio::test]
async fn an_oversized_answer_is_dropped_and_the_request_answered_empty() -> TestResult {
    let mut server = start();
    server.ready().await?;
    server.opened("echo").await?;
    let complete = ClientMessage::Complete {
        id: 5,
        position: position(0, 0),
    };
    server.page_sends(complete).await?;
    let asked = server.heard().await?;
    let big = "x".repeat(MAX_SERVER_MESSAGE_BYTES);
    let items = json!([{"label": big}]);
    server
        .says(json!({"jsonrpc": "2.0", "id": asked["id"], "result": items}))
        .await?;
    let expected = ServerMessage::Completion {
        id: 5,
        items: Vec::new(),
    };
    assert_eq!(server.page_receives().await?, expected);
    Ok(())
}
