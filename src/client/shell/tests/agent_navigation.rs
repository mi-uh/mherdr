use super::*;

fn agent_snapshot(count: usize) -> ClientShellSnapshot {
    let mut projected = snapshot();
    projected.panes = (1..=count.max(1))
        .map(|number| {
            let mut pane = projected.panes[0].clone();
            pane.pane_id = format!("pane_{number}");
            pane.focused = number == 1;
            pane
        })
        .collect();
    projected.agents = (1..=count)
        .map(|number| {
            let mut row = agent("agent", AgentStatus::Idle, number as u64);
            row.pane_id = format!("pane_{number}");
            row.focused = number == 1;
            row
        })
        .collect();
    projected
}

fn local_state(count: usize) -> ClientShellState {
    let config = Config::default();
    assert!(config.collect_diagnostics().is_empty());
    let mut state = ClientShellState::new(ClientShellConfig::from_config(&config));
    state.set_snapshot(Box::new(agent_snapshot(count)));
    state.set_pane_surface(surface());
    state.compose(110, 30).unwrap();
    state
}

fn preview(state: &mut ClientShellState, bytes: &[u8]) {
    let outcome = state.handle_input_bytes(bytes);
    assert!(outcome.requests.is_empty(), "{bytes:?}");
    assert!(outcome.actions.is_empty(), "{bytes:?}");
}

fn enter_agents(state: &mut ClientShellState) {
    preview(state, b"\x02");
    preview(state, b"a");
    assert_eq!(state.mode, ClientShellMode::Navigate);
    assert!(state.agent_navigation.is_some());
}

fn assert_agent(state: &ClientShellState, endpoint: &ClientEndpointId, pane: &str) {
    let target = state.selected_navigation_agent().expect("valid selection");
    assert_eq!(&target.workspace.endpoint_id, endpoint);
    assert_eq!(target.pane_id, pane);
}

#[test]
fn agent_navigation_previews_jk_and_only_enter_changes_focus() {
    let mut state = local_state(3);
    enter_agents(&mut state);
    assert_agent(&state, &ClientEndpointId::Local, "pane_1");
    preview(&mut state, b"j");
    assert_agent(&state, &ClientEndpointId::Local, "pane_2");
    preview(&mut state, b"k");
    assert_agent(&state, &ClientEndpointId::Local, "pane_1");
    preview(&mut state, b"\x1b[B");
    assert_agent(&state, &ClientEndpointId::Local, "pane_2");
    assert_eq!(
        state.snapshot.as_ref().unwrap().focused_pane_id.as_deref(),
        Some("pane_1")
    );

    let outcome = state.handle_input_bytes(b"\r");
    assert!(outcome.requests.is_empty());
    assert!(
        matches!(&outcome.actions[..], [ClientShellAction::Endpoint { request, .. }]
        if matches!(&request.method, crate::api::schema::Method::PaneFocus(target)
            if target.pane_id == "pane_2"))
    );
    assert_eq!(state.mode, ClientShellMode::Terminal);
    assert!(state.agent_navigation.is_none());
}

#[test]
fn agent_navigation_a_and_w_switch_sections_without_losing_workspace_preview() {
    let mut state = local_state(2);
    let mut projected = agent_snapshot(2);
    let mut second = projected.workspaces[0].clone();
    second.workspace_id = "ws_2".into();
    second.focused = false;
    projected.workspaces.push(second);
    state.set_snapshot(Box::new(projected));
    preview(&mut state, b"\x02w");
    preview(&mut state, b"j");
    let workspace = state.navigate_workspace_id.clone();
    assert_eq!(workspace.as_ref().unwrap().workspace_id, "ws_2");
    preview(&mut state, b"a");
    assert_agent(&state, &ClientEndpointId::Local, "pane_1");
    preview(&mut state, b"j");
    assert_agent(&state, &ClientEndpointId::Local, "pane_2");
    preview(&mut state, b"w");
    assert!(state.agent_navigation.is_none());
    assert_eq!(state.navigate_workspace_id, workspace);
    assert_eq!(state.mode, ClientShellMode::Navigate);
}

#[test]
fn agent_navigation_cancel_and_empty_list_never_send_pane_input() {
    for cancel in [b"\x1b".as_slice(), b"\x02", b"\x1b[91;5u"] {
        let mut state = local_state(2);
        enter_agents(&mut state);
        preview(&mut state, b"j");
        preview(&mut state, cancel);
        assert_eq!(state.mode, ClientShellMode::Terminal);
        assert!(state.agent_navigation.is_none());
    }
    let mut state = local_state(0);
    enter_agents(&mut state);
    for key in [b"j".as_slice(), b"k", b"\r", b"x"] {
        preview(&mut state, key);
        assert!(state.selected_navigation_agent().is_none());
    }
    preview(&mut state, b"w");
    assert!(state.agent_navigation.is_none());
    assert_eq!(state.mode, ClientShellMode::Navigate);
}

#[test]
fn agent_navigation_uses_filtered_order_and_stable_identity() {
    let mut state = local_state(3);
    let mut projected = agent_snapshot(3);
    projected.agent_view_label = Some("review".into());
    projected.agent_order = vec!["pane_3".into(), "pane_2".into()];
    state.set_snapshot(Box::new(projected.clone()));
    enter_agents(&mut state);
    assert_agent(&state, &ClientEndpointId::Local, "pane_3");
    preview(&mut state, b"j");
    assert_agent(&state, &ClientEndpointId::Local, "pane_2");
    projected.agent_order.reverse();
    state.set_snapshot(Box::new(projected));
    assert_agent(&state, &ClientEndpointId::Local, "pane_2");
    preview(&mut state, b"j");
    assert_agent(&state, &ClientEndpointId::Local, "pane_3");
}

#[test]
fn agent_navigation_rejects_removed_agents_and_reused_ids_after_reboot() {
    for reboot in [false, true] {
        let mut state = local_state(2);
        enter_agents(&mut state);
        preview(&mut state, b"j");
        let mut projected = agent_snapshot(2);
        if reboot {
            projected.boot_id = "new-boot".into();
        } else {
            projected.agents.pop();
        }
        state.set_snapshot(Box::new(projected));
        preview(&mut state, b"\r");
        assert_eq!(state.mode, ClientShellMode::Navigate);
        assert!(state.selected_navigation_agent().is_none());
        preview(&mut state, b"j");
        assert!(state.selected_navigation_agent().is_some());
    }
}

#[test]
fn agent_navigation_qualifies_identical_pane_ids_across_machines() {
    let (mut state, remote) = state_with_remote();
    state.set_snapshot(Box::new(agent_snapshot(1)));
    let mut projected = agent_snapshot(1);
    projected.boot_id = "remote-boot".into();
    state.set_endpoint_snapshot(&remote, Box::new(projected));
    state.compose(110, 30).unwrap();
    enter_agents(&mut state);
    preview(&mut state, b"j");
    assert_agent(&state, &remote, "pane_1");
    let outcome = state.handle_input_bytes(b"\r");
    assert!(
        matches!(&outcome.actions[..], [ClientShellAction::ActivateEndpoint {
        endpoint_id, target: Some(ClientEndpointFocusTarget::Pane(pane_id)),
    }] if endpoint_id == &remote && pane_id == "pane_1")
    );
}

#[test]
fn agent_navigation_skips_disconnected_machines_and_rejects_stale_selection() {
    let (mut state, remote) = state_with_remote();
    state.set_snapshot(Box::new(agent_snapshot(1)));
    let mut projected = agent_snapshot(1);
    projected.boot_id = "remote-boot".into();
    state.set_endpoint_snapshot(&remote, Box::new(projected));
    enter_agents(&mut state);
    preview(&mut state, b"j");
    assert_agent(&state, &remote, "pane_1");
    state.set_endpoint_status(&remote, ClientEndpointStatus::Reconnecting);
    preview(&mut state, b"\r");
    assert_eq!(state.mode, ClientShellMode::Navigate);
    preview(&mut state, b"j");
    assert_agent(&state, &ClientEndpointId::Local, "pane_1");
    preview(&mut state, b"j");
    assert_agent(&state, &ClientEndpointId::Local, "pane_1");
}

#[test]
fn agent_navigation_expands_a_collapsed_sidebar() {
    let mut state = local_state(2);
    state.sidebar_collapsed = true;
    let outcome = state.handle_input_bytes(b"\x02a");
    assert!(!state.sidebar_collapsed);
    assert!(outcome.resize);
    assert!(outcome.requests.is_empty());
    assert!(state.agent_navigation.is_some());
    // A fresh pane surface arrives after the resize request.
    state.set_pane_surface(surface());
    state.compose(110, 30).unwrap();
    assert!(state.hits.agents.iter().any(|(_, pane)| pane == "pane_1"));
}

#[test]
fn agent_navigation_scrolls_and_highlights_without_mouse_capture() {
    let mut state = local_state(20);
    state.config.palette.selection_bg = ratatui::style::Color::Rgb(30, 40, 50);
    enter_agents(&mut state);
    for _ in 0..19 {
        preview(&mut state, b"j");
    }
    let buffer = state.compose(110, 30).unwrap().to_ratatui_buffer().unwrap();
    let rect = state
        .hits
        .agents
        .iter()
        .find_map(|(rect, pane)| (pane == "pane_20").then_some(*rect))
        .expect("selected agent scrolled into view");
    assert!(state.agent_scroll > 0);
    assert_eq!(
        buffer[(rect.x, rect.y)].bg,
        state.config.palette.selection_bg
    );
    state.config.mouse_capture = false;
    let buffer = state.compose(110, 30).unwrap().to_ratatui_buffer().unwrap();
    assert_eq!(
        buffer[(rect.x, rect.y)].bg,
        state.config.palette.selection_bg
    );
}

#[test]
fn agent_navigation_mobile_reveals_selected_agent() {
    let mut state = local_state(20);
    state.compose(40, 20).unwrap();
    enter_agents(&mut state);
    for _ in 0..19 {
        preview(&mut state, b"j");
    }
    state.compose(40, 20).unwrap();
    assert!(state.hits.mobile_targets.iter().any(|(_, target)| {
        matches!(target, ClientMobileTarget::Agent { pane_id, .. } if pane_id == "pane_20")
    }));
    preview(&mut state, b"j");
    assert_agent(&state, &ClientEndpointId::Local, "pane_20");
}

/// Local state whose agents `pane_1..` have the given client-projected
/// statuses. Done needs an observed working -> idle transition, so done agents
/// are reported as working first and as idle in a second snapshot.
fn waiting_state(statuses: &[AgentStatus], focused: usize) -> ClientShellState {
    let mut state = local_state(statuses.len());
    let mut snapshot = agent_snapshot(statuses.len());
    snapshot.focused_pane_id = Some(format!("pane_{focused}"));
    let first = |status: AgentStatus| match status {
        AgentStatus::Done => AgentStatus::Working,
        status => status,
    };
    for (agent, status) in snapshot.agents.iter_mut().zip(statuses) {
        agent.agent_status = first(*status);
    }
    state.set_snapshot(Box::new(snapshot.clone()));
    snapshot.revision += 1;
    for (agent, status) in snapshot.agents.iter_mut().zip(statuses) {
        if *status == AgentStatus::Done {
            agent.agent_status = AgentStatus::Idle;
            agent.state_change_seq += 10;
        }
    }
    state.set_snapshot(Box::new(snapshot));
    for (number, status) in statuses.iter().enumerate() {
        let projected = state
            .snapshot
            .as_ref()
            .and_then(|snapshot| {
                snapshot
                    .agents
                    .iter()
                    .find(|agent| agent.pane_id == format!("pane_{}", number + 1))
            })
            .map(|agent| agent.agent_status);
        assert_eq!(projected, Some(*status), "pane_{}", number + 1);
    }
    state
}

fn focused_by_prefix_i(state: &mut ClientShellState) -> Option<String> {
    preview(state, b"\x02");
    let outcome = state.handle_input_bytes(b"i");
    assert!(outcome.requests.is_empty());
    match &outcome.actions[..] {
        [] => None,
        [ClientShellAction::Endpoint { request, .. }] => match &request.method {
            crate::api::schema::Method::PaneFocus(target) => Some(target.pane_id.clone()),
            other => panic!("unexpected method {other:?}"),
        },
        other => panic!("unexpected actions {other:?}"),
    }
}

#[test]
fn next_waiting_agent_prefers_blocked_then_done_and_skips_idle_and_working() {
    use AgentStatus::*;
    let mut state = waiting_state(&[Idle, Working, Done, Blocked], 1);
    assert_eq!(focused_by_prefix_i(&mut state).as_deref(), Some("pane_4"));
    assert_eq!(state.mode, ClientShellMode::Terminal);
}

#[test]
fn next_waiting_agent_cycles_from_the_focused_waiting_agent() {
    use AgentStatus::*;
    let mut state = waiting_state(&[Blocked, Idle, Done, Blocked], 1);
    // pane_1 is blocked and focused, so the next blocked agent comes first.
    assert_eq!(focused_by_prefix_i(&mut state).as_deref(), Some("pane_4"));

    let mut state = waiting_state(&[Blocked, Idle, Done, Blocked], 4);
    // After the last blocked agent the done ones follow.
    assert_eq!(focused_by_prefix_i(&mut state).as_deref(), Some("pane_3"));

    let mut state = waiting_state(&[Blocked, Idle, Done, Blocked], 3);
    // After the last waiting agent the list wraps around.
    assert_eq!(focused_by_prefix_i(&mut state).as_deref(), Some("pane_1"));
}

#[test]
fn next_waiting_agent_does_nothing_without_waiting_agents() {
    use AgentStatus::*;
    let mut state = waiting_state(&[Idle, Working, Idle], 2);
    assert_eq!(focused_by_prefix_i(&mut state), None);
    assert_eq!(state.mode, ClientShellMode::Terminal);
    assert_eq!(
        state
            .copy_feedback
            .as_ref()
            .map(|feedback| feedback.message.as_str()),
        Some(super::super::agent_navigation::NO_WAITING_AGENT_MESSAGE)
    );
    assert!(state.copy_feedback_deadline.is_some());
}

#[test]
fn i_in_agent_navigation_hints_when_nothing_waits() {
    use AgentStatus::*;
    let mut state = waiting_state(&[Idle, Working], 1);
    enter_agents(&mut state);
    preview(&mut state, b"i");
    assert_agent(&state, &ClientEndpointId::Local, "pane_1");
    assert_eq!(
        state
            .copy_feedback
            .as_ref()
            .map(|feedback| feedback.message.as_str()),
        Some(super::super::agent_navigation::NO_WAITING_AGENT_MESSAGE)
    );
}

#[test]
fn i_in_agent_navigation_selects_waiting_agents_without_focusing() {
    use AgentStatus::*;
    let mut state = waiting_state(&[Idle, Done, Working, Blocked], 1);
    enter_agents(&mut state);
    preview(&mut state, b"i");
    assert_agent(&state, &ClientEndpointId::Local, "pane_4");
    preview(&mut state, b"i");
    assert_agent(&state, &ClientEndpointId::Local, "pane_2");
    preview(&mut state, b"i");
    assert_agent(&state, &ClientEndpointId::Local, "pane_4");
    assert_eq!(
        state.snapshot.as_ref().unwrap().focused_pane_id.as_deref(),
        Some("pane_1")
    );
}

#[test]
fn i_in_workspace_navigation_switches_to_the_waiting_agent() {
    use AgentStatus::*;
    let mut state = waiting_state(&[Idle, Blocked], 1);
    preview(&mut state, b"\x02");
    preview(&mut state, b"w");
    assert!(state.agent_navigation.is_none());
    preview(&mut state, b"i");
    assert!(state.agent_navigation.is_some());
    assert_agent(&state, &ClientEndpointId::Local, "pane_2");
}
