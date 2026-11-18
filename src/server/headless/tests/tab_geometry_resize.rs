use super::*;

fn populated_geometry_server(count: usize, alternate: bool) -> HeadlessServer {
    let mut server = test_headless_server();
    let mut workspace = crate::workspace::Workspace::test_new("geometry");
    let root = workspace.tabs[0].root_pane;
    let mut ids = vec![root];
    for index in 1..count {
        workspace.tabs[0].layout.focus_pane(ids[(index - 1) / 2]);
        ids.push(workspace.test_split(if index % 2 == 0 {
            ratatui::layout::Direction::Vertical
        } else {
            ratatui::layout::Direction::Horizontal
        }));
    }
    let mut bytes = "row 界 e\u{301}\r\n".repeat(120).into_bytes();
    if alternate {
        bytes.extend_from_slice(b"\x1b[?1049hALT");
    }
    for id in ids {
        workspace.insert_test_runtime(
            id,
            crate::terminal::TerminalRuntime::test_with_scrollback_bytes(80, 24, 65536, &bytes),
        );
    }
    server.app.state.workspaces = vec![workspace];
    server.app.state.active = Some(0);
    server.app.state.pane_scrollbars = true;
    server
}

type RuntimeGeometry = ((u16, u16), Option<(u32, u32)>);

fn geometry_contract(server: &HeadlessServer) -> Vec<RuntimeGeometry> {
    server.app.state.workspaces[0].tabs[0]
        .layout
        .panes(Rect::new(0, 0, 120, 40))
        .iter()
        .map(|pane| {
            let runtime = server
                .app
                .state
                .runtime_for_pane_in_workspace(&server.app.terminal_runtimes, 0, pane.id)
                .unwrap();
            (runtime.current_size(), runtime.pixel_size())
        })
        .collect()
}

#[tokio::test]
async fn resize_only_tab_surface_matches_complete_layout_resize() {
    let target = crate::ui::TabSurfaceTarget {
        workspace_index: 0,
        tab_index: 0,
    };
    for count in [1, 15] {
        for alternate in [false, true] {
            for zoomed in [false, true] {
                for locked in [false, true] {
                    let mut full = populated_geometry_server(count, alternate);
                    let mut narrow = populated_geometry_server(count, alternate);
                    for server in [&mut full, &mut narrow] {
                        server.app.state.workspaces[0].tabs[0].zoomed = zoomed;
                        if locked {
                            let tab = &server.app.state.workspaces[0].tabs[0];
                            server
                                .app
                                .state
                                .direct_attach_resize_locks
                                .insert(tab.terminal_id(tab.layout.focused()).unwrap().clone());
                        }
                    }
                    for area in [Rect::new(0, 0, 120, 40), Rect::new(0, 0, 7, 3)] {
                        let cell_size = crate::kitty_graphics::HostCellSize {
                            width_px: 8,
                            height_px: 16,
                        };
                        crate::ui::compute_tab_surface_for(
                            &full.app.state,
                            &full.app.terminal_runtimes,
                            Some(target),
                            area,
                            true,
                            cell_size,
                        );
                        crate::ui::resize_tab_surface(
                            &narrow.app.state,
                            &narrow.app.terminal_runtimes,
                            0,
                            0,
                            area,
                            cell_size,
                        );
                        assert_eq!(geometry_contract(&full), geometry_contract(&narrow));
                    }
                    shutdown_test_runtimes(&mut full);
                    shutdown_test_runtimes(&mut narrow);
                }
            }
        }
    }
}

#[tokio::test]
#[ignore = "local geometry-stage profile, not a wall-clock CI assertion"]
async fn tab_geometry_profile() {
    let target = crate::ui::TabSurfaceTarget {
        workspace_index: 0,
        tab_index: 0,
    };
    for count in [1, 15] {
        let mut server = populated_geometry_server(count, false);
        let area = Rect::new(0, 0, 120, 40);
        let cell_size = crate::kitty_graphics::HostCellSize::default();
        crate::ui::resize_tab_surface(
            &server.app.state,
            &server.app.terminal_runtimes,
            0,
            0,
            area,
            cell_size,
        );
        for complete in [true, false, false, true] {
            let mut samples = Vec::new();
            for index in 0..1010 {
                let started = Instant::now();
                if complete {
                    std::hint::black_box(crate::ui::compute_tab_surface_for(
                        &server.app.state,
                        &server.app.terminal_runtimes,
                        Some(target),
                        area,
                        true,
                        cell_size,
                    ));
                } else {
                    crate::ui::resize_tab_surface(
                        &server.app.state,
                        &server.app.terminal_runtimes,
                        0,
                        0,
                        area,
                        cell_size,
                    );
                }
                if index >= 10 {
                    samples.push(started.elapsed().as_nanos());
                }
            }
            samples.sort_unstable();
            println!(
                "tab_geometry panes={count} complete={complete} median_ns={} p95_ns={}",
                samples[500], samples[949]
            );
        }
        shutdown_test_runtimes(&mut server);
    }
}
