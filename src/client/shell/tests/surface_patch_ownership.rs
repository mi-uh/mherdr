use super::*;

fn fixture(cols: u16, rows: u16, pane_count: usize, mode: &str) -> ClientShellState {
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&Config::default()));
    state.set_snapshot(Box::new(snapshot()));
    let area = state.layout(cols, rows).pane_surface;
    let mut next = surface();
    next.frame = FrameData::from_ratatui_buffer_with_hyperlinks(
        &Buffer::empty(Rect::new(0, 0, area.width, area.height)),
        None,
        &[],
    );
    let template = next.panes[0].clone();
    next.panes = (0..pane_count)
        .map(|index| {
            let mut pane = template.clone();
            pane.pane_id = format!("pane_{}", index + 1);
            let top = area.height as usize * index / pane_count;
            let bottom = area.height as usize * (index + 1) / pane_count;
            pane.rect = SurfaceRect {
                x: 0,
                y: top as u16,
                width: area.width,
                height: (bottom - top) as u16,
            };
            pane.inner_rect = pane.rect;
            pane.focused = index == 0;
            pane
        })
        .collect();
    state.set_pane_surface(next);
    state.compose(cols, rows).expect("initial composition");
    match mode {
        "selection" | "other_selection" => {
            let selected = if mode == "other_selection" {
                pane_count
            } else {
                1
            };
            state.selection = Some(crate::selection::Selection::absolute_range(
                format!("pane_{selected}"),
                (0, 0),
                (0, 3),
            ));
        }
        "overlay" => {
            state.overlay = Some(ClientShellOverlay::Help(ClientHelpOverlay {
                query: TextEditor::default(),
                search_focused: false,
                scroll: 0,
            }));
        }
        _ => {}
    }
    state.compose(cols, rows).expect("fixture composition");
    state
}

fn patch(state: &ClientShellState, index: usize) -> crate::protocol::PaneSurfacePatch {
    let current = state.pane_surface.as_ref().unwrap();
    let pane = current.panes[0].clone();
    let cell = crate::protocol::CellData {
        symbol: if index.is_multiple_of(2) {
            "x"
        } else {
            "e\u{301}"
        }
        .into(),
        fg: 0,
        bg: 0,
        modifier: 0,
        skip: false,
        hyperlink: None,
    };
    crate::protocol::PaneSurfacePatch {
        boot_id: current.boot_id.clone(),
        projection_revision: current.projection_revision,
        base_surface_revision: current.surface_revision,
        surface_revision: current.surface_revision + 1,
        rows: (pane.inner_rect.y..pane.inner_rect.y + pane.inner_rect.height)
            .map(|y| crate::protocol::PaneSurfacePatchRow {
                x: pane.inner_rect.x,
                y,
                cells: vec![cell.clone(); pane.inner_rect.width as usize],
            })
            .collect(),
        panes: vec![pane],
        cursor: None,
    }
}

#[test]
fn retained_patch_transfers_owned_rows_and_unicode_strings_to_host_output() {
    let mut state = fixture(120, 40, 1, "plain");
    let update = patch(&state, 1);
    let row_storage = update.rows.as_ptr();
    let cells = update.rows[0].cells.as_ptr();
    let symbol = update.rows[0].cells[0].symbol.as_ptr();
    let expected = update.rows[0].cells[0].clone();
    let area = state.layout(120, 40).pane_surface;
    let ClientPaneSurfacePatchOutcome::Applied(Some(output)) =
        state.apply_pane_surface_patch(update)
    else {
        panic!("expected retained patch");
    };
    assert_eq!(output.rows.as_ptr(), row_storage);
    assert_eq!(output.rows[0].cells.as_ptr(), cells);
    assert_eq!(output.rows[0].cells[0].symbol.as_ptr(), symbol);
    assert_eq!(output.rows[0].x, area.x);
    assert_eq!(output.rows[0].y, area.y);
    assert_eq!(
        state.pane_surface.as_ref().unwrap().frame.cells[0],
        expected
    );
}

#[test]
fn other_pane_patches_preserve_selection_and_match_full_composition() {
    for scenario in 0..12 {
        let phase = scenario / 4;
        let change = ["text", "metadata", "cursor", "hidden_cursor"][scenario % 4];
        let mut state = fixture(120, 40, 2, "plain");
        let mut selection =
            crate::selection::Selection::absolute_anchor("pane_2".to_owned(), (0, 0));
        if phase > 0 {
            selection =
                crate::selection::Selection::absolute_range("pane_2".to_owned(), (0, 0), (0, 3));
        }
        if phase == 2 {
            selection.finish();
        }
        state.selection = Some(selection);
        let composed = state.compose(120, 40).unwrap();
        let selected_hit = state.hits.panes[1].inner_rect;
        let selection_before = state.selection.as_ref().unwrap().ordered_cells();
        let composed_at = state.last_composed_at;
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(30);
        state.selection_repaint_deadline = Some(deadline);
        let mut update = patch(&state, 1);
        if change == "metadata" {
            update.rows.clear();
            update.panes[0].content_revision = 2;
            update.panes[0].scroll = Some(crate::protocol::PaneSurfaceScrollMetrics {
                offset_from_bottom: 2,
                max_offset_from_bottom: 10,
                viewport_rows: update.panes[0].inner_rect.height as u64,
            });
        } else if change == "cursor" {
            update.cursor = Some(crate::protocol::CursorState {
                x: 2,
                y: state.pane_surface.as_ref().unwrap().panes[1].inner_rect.y,
                visible: true,
                shape: 0,
            });
        } else if change == "hidden_cursor" {
            update.cursor = Some(crate::protocol::CursorState {
                x: 1,
                y: 0,
                visible: false,
                shape: 0,
            });
        }
        let ClientPaneSurfacePatchOutcome::Applied(Some(output)) =
            state.apply_pane_surface_patch(update)
        else {
            panic!("unrelated pane should keep retained output");
        };
        let patched = apply_composed_surface_patch(&composed, output).unwrap();
        assert_eq!(
            state.selection.as_ref().unwrap().ordered_cells(),
            selection_before
        );
        assert_eq!(state.selection.as_ref().unwrap().is_visible(), phase > 0);
        assert_eq!(state.selection.as_ref().unwrap().is_finalized(), phase == 2);
        assert_eq!(state.hits.panes[1].inner_rect, selected_hit);
        assert_eq!(state.selection_repaint_deadline, Some(deadline));
        assert_eq!(state.last_composed_at, composed_at);
        let full = state.compose(120, 40).unwrap();
        assert_eq!(patched.cells, full.cells);
        assert_eq!(patched.cursor, full.cursor);
    }
}

#[test]
fn selected_pane_metadata_and_other_blockers_still_require_recomposition() {
    for blocker in ["selection", "overlay", "deadline", "mixed"] {
        let mut state = fixture(120, 40, 2, "selection");
        let mut update = patch(&state, 0);
        update.rows.clear();
        if blocker == "overlay" {
            state = fixture(120, 40, 2, "overlay");
            update = patch(&state, 0);
        } else if blocker == "deadline" {
            state.selection = None;
            state.selection_highlight_clear_deadline = Some(std::time::Instant::now());
        } else if blocker == "mixed" {
            update
                .panes
                .push(state.pane_surface.as_ref().unwrap().panes[1].clone());
        }
        assert!(
            matches!(
                state.apply_pane_surface_patch(update),
                ClientPaneSurfacePatchOutcome::Applied(None)
            ),
            "{blocker}"
        );
    }
}

#[test]
#[ignore = "local performance profile, not a wall-clock CI assertion"]
fn client_surface_patch_profile() {
    for (cols, rows) in [(120, 40), (240, 80)] {
        for count in [1, 15] {
            for mode in ["plain", "selection", "other_selection", "overlay"] {
                if mode == "other_selection" && count == 1 {
                    continue;
                }
                let mut state = fixture(cols, rows, count, mode);
                let mut samples = Vec::new();
                let mut fast = 0;
                for index in 0..110 {
                    let update = patch(&state, index);
                    let started = std::time::Instant::now();
                    let outcome = state.apply_pane_surface_patch(update);
                    let frame = match &outcome {
                        ClientPaneSurfacePatchOutcome::Applied(Some(_)) => {
                            fast += usize::from(index >= 10);
                            None
                        }
                        ClientPaneSurfacePatchOutcome::Applied(None) => state.compose(cols, rows),
                        ClientPaneSurfacePatchOutcome::Rejected => panic!("valid fixture rejected"),
                    };
                    let elapsed = started.elapsed().as_nanos();
                    std::hint::black_box((outcome, frame));
                    if index >= 10 {
                        samples.push(elapsed);
                    }
                }
                samples.sort_unstable();
                println!(
                    "client_patch {cols}x{rows} panes={count} mode={mode} median_ns={} p95_ns={} fast={fast}/100",
                    samples[50], samples[94]
                );
            }
        }
    }
}
