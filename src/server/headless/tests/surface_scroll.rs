use super::*;

fn scrolling_lines(range: std::ops::Range<usize>) -> Vec<u8> {
    range
        .map(|n| {
            format!(
                "build step {n:>3}: compiled module_{n} in {}ms\r\n",
                n * 7 % 97
            )
        })
        .collect::<String>()
        .into_bytes()
}

fn apply_rows(frame: &mut FrameData, rows: &[crate::protocol::PaneSurfacePatchRow]) {
    for row in rows {
        let start = usize::from(row.y) * usize::from(frame.width) + usize::from(row.x);
        frame.cells[start..start + row.cells.len()].clone_from_slice(&row.cells);
    }
}

fn scrollback_test_server() -> (
    HeadlessServer,
    std::sync::mpsc::Receiver<Vec<u8>>,
    std::sync::mpsc::Receiver<Vec<u8>>,
    crate::layout::PaneId,
) {
    let mut server = test_headless_server();
    let mut workspace = crate::workspace::Workspace::test_new("scrollback");
    let pane_id = workspace.focused_pane_id().expect("focused pane");
    let history = (0..200)
        .map(|n| format!("row {n:03} \x1b[32mwide 界 e\u{301}\x1b[0m\r\n"))
        .collect::<String>();
    workspace.insert_test_runtime(
        pane_id,
        crate::terminal::TerminalRuntime::test_with_scrollback_bytes(
            80,
            23,
            1024 * 1024,
            history.as_bytes(),
        ),
    );
    server.app.state.workspaces = vec![workspace];
    server.app.state.active = Some(0);
    server.app.state.selected = 0;
    server.app.state.mode = crate::app::Mode::Terminal;
    server.app.state.pane_scrollbars = true;
    let (control, render) = connect_matching_test_shell(&mut server, 7);
    (server, control, render, pane_id)
}

fn assert_same_presentation(retained: &FrameData, full: &FrameData) {
    assert_eq!((retained.width, retained.height), (full.width, full.height));
    assert_eq!(retained.cursor, full.cursor);
    assert_eq!(retained.hyperlinks, full.hyperlinks);
    assert_eq!(retained.graphics, full.graphics);
    // The existing collectors differ in the unpainted trailing cell of a wide
    // grapheme (empty/styled versus blank/default). Compare exact host output,
    // not those covered internal cells; do not normalize any visible content.
    let encoder = crate::protocol::render_ansi::BlitEncoder::new();
    assert_eq!(
        encoder.encode(retained, true).bytes,
        encoder.encode(full, true).bytes
    );
}

#[tokio::test]
async fn scrollback_retained_rows_match_full_render_in_both_directions() {
    let (mut server, _control, render, pane_id) = scrollback_test_server();
    server.render_and_stream();
    let _ = recv_pane_surface(&render, "baseline");
    for offset in [3, 9, 6, 0] {
        server
            .app
            .state
            .runtime_for_pane_in_workspace(&server.app.terminal_runtimes, 0, pane_id)
            .unwrap()
            .set_scroll_offset_from_bottom(offset);
        assert!(server.render_retained_pane_surface_and_stream(&HashSet::from([pane_id])));
        let patch = recv_pane_surface_patch(&render, "viewport patch");
        assert_eq!(
            patch.panes[0].scroll.unwrap().offset_from_bottom,
            offset as u64
        );
        let retained = server.clients[&7]
            .render_state
            .last_pane_surface()
            .unwrap()
            .clone();
        server
            .clients
            .get_mut(&7)
            .unwrap()
            .render_state
            .request_repaint();
        server.render_and_stream();
        let full = recv_pane_surface(&render, "full reference");
        assert_same_presentation(&retained.frame, &full.frame);
        assert_eq!(retained.panes, full.panes, "offset {offset}");
        assert_eq!(retained.splits, full.splits);
        assert_eq!(retained.graphics, full.graphics);
    }
    shutdown_test_runtimes(&mut server);
}

fn wheel_event(server: &HeadlessServer, pane_id: crate::layout::PaneId, up: bool) -> ServerEvent {
    ServerEvent::ClientShellPaneInput {
        client_id: 7,
        pane_id: server.app.public_pane_id(0, pane_id).unwrap(),
        events: vec![protocol::ClientPaneInputEvent::Mouse {
            kind: if up {
                protocol::ClientMouseKind::ScrollUp
            } else {
                protocol::ClientMouseKind::ScrollDown
            },
            position: protocol::ClientMousePosition::Cell { column: 2, row: 1 },
            geometry: None,
            modifiers: 0,
            lines: 3,
        }],
    }
}

#[tokio::test]
async fn scrollback_wheel_queues_viewport_damage_and_wakes_mid_loop_drain() {
    let (mut server, _control, render, pane_id) = scrollback_test_server();
    server.render_and_stream();
    let _ = recv_pane_surface(&render, "baseline");
    server.app.render_dirty.take();
    // Model client events arriving after the loop's initial pending-signal check.
    assert!(!server.app.render_dirty.is_pending());
    server
        .server_event_tx
        .try_send(wheel_event(&server, pane_id, true))
        .unwrap();
    assert!(!server.drain_server_events(), "no full render needed");
    tokio::time::timeout(
        Duration::from_millis(100),
        server.app.render_notify.notified(),
    )
    .await
    .expect("viewport input must wake the loop without another event");
    assert!(server.has_pending_presentation_work(false, false));
    server.note_interactive_input(pane_id, Instant::now());
    assert!(!server.can_render_interactive(Instant::now()));
    let request = server.app.render_dirty.take();
    assert!(!request.generic);
    assert!(request.pty_sources.is_empty());
    assert_eq!(request.viewport_sources, HashSet::from([pane_id]));
    assert!(server.render_retained_pane_surface_and_stream(&request.viewport_sources));
    let patch = recv_pane_surface_patch(&render, "wheel patch");
    assert_eq!(patch.panes[0].scroll.unwrap().offset_from_bottom, 3);

    // A down event returns to live output; another down event is a no-op.
    assert!(!server.handle_server_event(wheel_event(&server, pane_id, false)));
    server.app.render_dirty.take();
    assert!(!server.handle_server_event(wheel_event(&server, pane_id, false)));
    assert!(!server.app.render_dirty.is_pending());
    shutdown_test_runtimes(&mut server);
}

#[tokio::test]
async fn scrollback_wheel_keeps_foreground_and_generic_changes_full() {
    let (mut server, _control, render, pane_id) = scrollback_test_server();
    server.render_and_stream();
    let _ = recv_pane_surface(&render, "baseline");
    server.app.render_dirty.take();
    let (_other_control, _other_render) = connect_matching_test_shell(&mut server, 8);
    // The same wheel input now also promotes client 7, requiring a full render.
    assert!(server.handle_server_event(wheel_event(&server, pane_id, true)));
    assert!(server.app.render_dirty.take().viewport_sources.is_empty());
    server.app.render_dirty.request_generic();
    assert!(!server.handle_server_event(wheel_event(&server, pane_id, true)));
    let request = server.app.render_dirty.take();
    assert!(
        request.generic,
        "viewport damage must not erase full-render work"
    );
    assert_eq!(request.viewport_sources, HashSet::from([pane_id]));
    shutdown_test_runtimes(&mut server);
}

#[tokio::test]
async fn scrollback_retained_damage_reaches_differently_sized_viewers() {
    let (mut server, _control, render, pane_id) = scrollback_test_server();
    let (_small_control, small_render) = connect_test_shell(&mut server, 8, 40, 12);
    // Promotion/geometry ownership stays on the complete renderer.
    assert!(server.handle_server_event(wheel_event(&server, pane_id, true)));
    server.render_and_stream();
    let _ = recv_pane_surface(&render, "large baseline");
    let _ = recv_pane_surface(&small_render, "small baseline");
    server.app.render_dirty.take();
    assert!(!server.handle_server_event(wheel_event(&server, pane_id, true)));
    let request = server.app.render_dirty.take();
    assert!(server.render_retained_pane_surface_and_stream(&request.viewport_sources));
    let _ = recv_pane_surface_patch(&render, "large viewport patch");
    let _ = recv_pane_surface_patch(&small_render, "small viewport patch");
    let retained: Vec<_> = [7, 8]
        .iter()
        .map(|id| {
            server.clients[id]
                .render_state
                .last_pane_surface()
                .unwrap()
                .clone()
        })
        .collect();
    for id in [7, 8] {
        server
            .clients
            .get_mut(&id)
            .unwrap()
            .render_state
            .request_repaint();
    }
    server.render_and_stream();
    for (before, receiver) in retained.iter().zip([&render, &small_render]) {
        let full = recv_pane_surface(receiver, "full viewer reference");
        assert_same_presentation(&before.frame, &full.frame);
        assert_eq!(before.panes, full.panes);
    }
    shutdown_test_runtimes(&mut server);
}

#[tokio::test]
async fn scrollback_retained_damage_preserves_hyperlink_and_synchronized_fallbacks() {
    let (mut server, _control, render, pane_id) = scrollback_test_server();
    write_shared_test_pane(
        &mut server,
        pane_id,
        b"\r\x1b]8;;https://example.com\x1b\\LINK\x1b]8;;\x1b\\",
    );
    server.render_and_stream();
    let initial = recv_pane_surface(&render, "hyperlinked baseline");
    assert!(!initial.frame.hyperlinks.is_empty());
    server.app.render_dirty.take();
    assert!(!server.handle_server_event(wheel_event(&server, pane_id, true)));
    let request = server.app.render_dirty.take();
    assert!(!server.render_retained_pane_surface_and_stream(&request.viewport_sources));
    server.render_and_stream();
    let _ = recv_pane_surface(&render, "hyperlink fallback");

    write_shared_test_pane(&mut server, pane_id, b"\x1b[?2026h\rPARTIAL");
    assert!(!server.handle_server_event(wheel_event(&server, pane_id, true)));
    let request = server.app.render_dirty.take();
    assert!(!server.render_retained_pane_surface_and_stream(&request.viewport_sources));
    server.render_and_stream();
    assert!(
        render.try_recv().is_err(),
        "must not publish a partial frame"
    );
    write_shared_test_pane(&mut server, pane_id, b"\x1b[?2026l");
    server.render_and_stream();
    let _ = recv_pane_surface(&render, "synchronized recovery");
    shutdown_test_runtimes(&mut server);
}

#[tokio::test]
async fn surface_scroll_sends_scrolling_output_as_a_shift_and_new_rows() {
    let (mut server, _control_rx, render_rx, pane_id) =
        retained_test_server_with_control(&scrolling_lines(0..40));
    server
        .clients
        .get_mut(&1)
        .expect("scroll client")
        .render_state
        .enable_surface_scroll(true);
    server.render_and_stream();
    let mut decoder = protocol::surface_reuse::Decoder::new(false, true);
    let ServerMessage::PaneSurface(mut shell) = decoder
        .decode(read_server_message(
            render_rx.recv_timeout(Duration::from_secs(1)).unwrap(),
        ))
        .expect("initial surface")
    else {
        panic!("expected the initial pane surface");
    };

    write_shared_test_pane(&mut server, pane_id, &scrolling_lines(40..42));
    assert!(server.render_retained_pane_surface_and_stream(&HashSet::from([pane_id])));
    let bytes = render_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    let message = read_server_message(bytes.clone());
    assert!(matches!(
        &message,
        ServerMessage::EndpointControl { kind, .. } if kind == protocol::surface_scroll::MESSAGE_KIND
    ));

    // The decoder hands the client shell an ordinary patch that reaches the
    // server's committed surface exactly.
    let ServerMessage::PaneSurfacePatch(patch) = decoder.decode(message).expect("scroll decode")
    else {
        panic!("expected an expanded pane patch");
    };
    apply_rows(&mut shell.frame, &patch.rows);
    let committed = server.clients[&1]
        .render_state
        .last_pane_surface()
        .expect("committed surface");
    assert_eq!(shell.frame.cells, committed.frame.cells);
    assert!(frame_text(&shell.frame).contains("module_41"));

    let expanded = HeadlessServer::frame_server_message(&ServerMessage::PaneSurfacePatch(patch))
        .expect("expanded frame");
    assert!(
        bytes.len() * 4 < expanded.len(),
        "scroll frame {} should be far smaller than the rows it replaces {}",
        bytes.len(),
        expanded.len()
    );
    shutdown_test_runtimes(&mut server);
}

#[tokio::test]
async fn surface_scroll_is_not_sent_to_a_peer_that_did_not_negotiate_it() {
    let (mut server, _control_rx, render_rx, pane_id) =
        retained_test_server_with_control(&scrolling_lines(0..40));
    server.render_and_stream();
    let _ = render_rx.recv_timeout(Duration::from_secs(1)).unwrap();

    write_shared_test_pane(&mut server, pane_id, &scrolling_lines(40..42));
    assert!(server.render_retained_pane_surface_and_stream(&HashSet::from([pane_id])));
    let message = read_server_message(render_rx.recv_timeout(Duration::from_secs(1)).unwrap());
    assert!(matches!(message, ServerMessage::PaneSurfacePatch(_)));
    shutdown_test_runtimes(&mut server);
}
