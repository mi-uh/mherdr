//! Client-only agent selection; moving the cursor never changes runtime focus.

use super::*;
use crossterm::event::KeyModifiers;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AgentNavigationTarget {
    pub(super) workspace: WorkspaceNavigationTarget,
    pub(super) pane_id: String,
}

pub(super) struct AgentNavigation {
    // Keep the section active even when there are no agents to select.
    pub(super) selected: Option<AgentNavigationTarget>,
    reveal: bool,
}

impl ClientShellState {
    fn agent_navigation_target(
        &self,
        endpoint_id: &ClientEndpointId,
        pane_id: &str,
    ) -> Option<AgentNavigationTarget> {
        let endpoint = self
            .endpoints
            .iter()
            .find(|entry| &entry.endpoint_id == endpoint_id)?;
        let agent = endpoint
            .snapshot
            .as_deref()?
            .agents
            .iter()
            .find(|agent| agent.pane_id == pane_id)?;
        Some(AgentNavigationTarget {
            workspace: self.navigation_target(endpoint_id, &agent.workspace_id)?,
            pane_id: pane_id.to_owned(),
        })
    }

    fn agent_navigation_target_valid(&self, target: &AgentNavigationTarget) -> bool {
        self.navigation_target_valid(&target.workspace)
            && self.endpoints.iter().any(|endpoint| {
                endpoint.endpoint_id == target.workspace.endpoint_id
                    && endpoint.snapshot.as_deref().is_some_and(|snapshot| {
                        snapshot.agents.iter().any(|agent| {
                            agent.pane_id == target.pane_id
                                && agent.workspace_id == target.workspace.workspace_id
                        })
                    })
            })
    }

    pub(super) fn selected_navigation_agent(&self) -> Option<&AgentNavigationTarget> {
        if self.mode != ClientShellMode::Navigate {
            return None;
        }
        self.agent_navigation
            .as_ref()?
            .selected
            .as_ref()
            .filter(|target| self.agent_navigation_target_valid(target))
    }

    pub(super) fn begin_agent_navigation(&mut self, outcome: &mut ClientShellInput) {
        self.pending_workspace_highlight = None;
        self.mode = ClientShellMode::Navigate;
        let targets = super::aggregate_navigation::online_agent_targets(
            &self.endpoints,
            &self.active_endpoint_id,
            self.config.agent_panel_sort,
        );
        let focused_pane = self
            .snapshot
            .as_deref()
            .and_then(|snapshot| snapshot.focused_pane_id.as_deref());
        let target = targets
            .iter()
            .find(|target| {
                target.endpoint_id == self.active_endpoint_id
                    && Some(target.pane_id.as_str()) == focused_pane
            })
            .or_else(|| targets.first());
        let selected = target
            .and_then(|target| self.agent_navigation_target(&target.endpoint_id, &target.pane_id));
        self.agent_navigation = Some(AgentNavigation {
            selected,
            reveal: true,
        });
        if self.sidebar_collapsed && !self.mobile_layout_active() {
            // Reuse normal expansion, including surface invalidation and preferences.
            self.record_binding(
                crate::input::KeybindMatch::Action(crate::input::KeybindAction::ToggleSidebar),
                outcome,
            );
        }
        outcome.repaint = true;
    }

    pub(super) fn prepare_agent_navigation_layout(&mut self, cols: u16, rows: u16) {
        if self.mode != ClientShellMode::Navigate {
            self.agent_navigation = None;
            return;
        }
        let resized = self.last_composed_size != Some((cols, rows));
        if self
            .agent_navigation
            .as_ref()
            .is_none_or(|navigation| !navigation.reveal && !resized)
        {
            return;
        }
        if let Some(target) = self.selected_navigation_agent().cloned() {
            let layout = self.layout(cols, rows);
            let (_, agents) =
                crate::ui::expanded_sidebar_sections(layout.sidebar, self.sidebar_section_split);
            self.reveal_endpoint_agent(
                &target.workspace.endpoint_id,
                &target.pane_id,
                agents.height.saturating_sub(3),
            );
            self.reveal_mobile_workspace = true;
        }
        if let Some(navigation) = self.agent_navigation.as_mut() {
            navigation.reveal = false;
        }
    }

    fn move_navigation_agent(&mut self, delta: isize) {
        let targets = super::aggregate_navigation::online_agent_targets(
            &self.endpoints,
            &self.active_endpoint_id,
            self.config.agent_panel_sort,
        );
        let current = self.selected_navigation_agent().and_then(|selected| {
            targets.iter().position(|target| {
                target.endpoint_id == selected.workspace.endpoint_id
                    && target.pane_id == selected.pane_id
            })
        });
        let next = match current {
            Some(index) if self.mobile_layout_active() => {
                (index as isize + delta).clamp(0, targets.len() as isize - 1) as usize
            }
            Some(index) => (index as isize + delta).rem_euclid(targets.len() as isize) as usize,
            None if delta < 0 => targets.len().saturating_sub(1),
            None => 0,
        };
        let selected = targets
            .get(next)
            .and_then(|target| self.agent_navigation_target(&target.endpoint_id, &target.pane_id));
        self.agent_navigation = Some(AgentNavigation {
            selected,
            reveal: true,
        });
    }

    pub(super) fn route_agent_navigation_key(
        &mut self,
        key: &crate::input::TerminalKey,
        outcome: &mut ClientShellInput,
    ) {
        let (code, modifiers) = crate::config::normalize_key_combo((key.code, key.modifiers));
        if code == KeyCode::Enter && modifiers.is_empty() {
            let selected = self
                .agent_navigation
                .as_ref()
                .and_then(|navigation| navigation.selected.clone());
            let Some(target) = selected else {
                return;
            };
            if !self.agent_navigation_target_valid(&target) {
                self.receive_endpoint_unavailable(
                    "Agent is no longer available; select a connected agent".into(),
                );
                outcome.repaint = true;
                return;
            }
            if self.focus_or_activate(
                target.workspace.endpoint_id,
                ClientEndpointFocusTarget::Pane(target.pane_id),
                outcome,
            ) {
                self.mode = ClientShellMode::Terminal;
                self.agent_navigation = None;
                self.navigate_workspace_id = None;
            }
            outcome.repaint = true;
        } else if self
            .config
            .keybinds
            .keybinds
            .navigate
            .workspace_up
            .matches_direct_key(key)
        {
            self.move_navigation_agent(-1);
            outcome.repaint = true;
        } else if self
            .config
            .keybinds
            .keybinds
            .navigate
            .workspace_down
            .matches_direct_key(key)
        {
            self.move_navigation_agent(1);
            outcome.repaint = true;
        } else if key.code == KeyCode::Char('[') && key.modifiers == KeyModifiers::CONTROL {
            self.mode = self.copy_or_terminal_mode();
            self.agent_navigation = None;
            self.navigate_workspace_id = None;
            outcome.repaint = true;
        } else if let Some(
            binding @ crate::input::KeybindMatch::Action(crate::input::KeybindAction::Help),
        ) = crate::input::resolve_prefix_binding(&self.config.keybinds.keybinds, key)
        {
            self.record_binding(binding, outcome);
        }
    }
}

/// Only inspect already-rendered sidebar hit rectangles, never terminal state.
pub(super) fn render_agent_navigation_highlight(
    buffer: &mut Buffer,
    hits: &ShellHitMap,
    selected: Option<&AgentNavigationTarget>,
    active_endpoint_id: &ClientEndpointId,
    palette: &Palette,
) {
    let Some(selected) = selected else {
        return;
    };
    let rect = hits
        .endpoint_agents
        .iter()
        .find_map(|(rect, endpoint, pane)| {
            (endpoint == &selected.workspace.endpoint_id && pane == &selected.pane_id)
                .then_some(*rect)
        })
        .or_else(|| {
            if &selected.workspace.endpoint_id == active_endpoint_id {
                hits.agents
                    .iter()
                    .find_map(|(rect, pane)| (pane == &selected.pane_id).then_some(*rect))
            } else {
                None
            }
        });
    if let Some(rect) = rect {
        let background = if palette.selection_bg == ratatui::style::Color::Reset {
            palette.active_row_bg
        } else {
            palette.selection_bg
        };
        buffer.set_style(rect, Style::default().bg(background));
    }
}

pub(super) const NO_WAITING_AGENT_MESSAGE: &str = "no agent waiting";

/// Agents that wait for the user, in sidebar order with blocked agents first.
/// Done uses this client's projected status, so a viewed completion drops out.
fn waiting_agent_targets(
    endpoints: &[ClientShellEndpoint],
    active_endpoint_id: &ClientEndpointId,
    sort: crate::config::AgentPanelSortConfig,
) -> Vec<super::aggregate_navigation::AggregateAgentTarget> {
    use crate::api::schema::AgentStatus;
    let mut rows =
        super::aggregate_navigation::aggregate_agent_rows(endpoints, active_endpoint_id, sort)
            .into_iter()
            .filter(|row| !row.endpoint.stale())
            .filter(|row| {
                matches!(
                    row.agent.agent_status,
                    AgentStatus::Blocked | AgentStatus::Done
                )
            })
            .collect::<Vec<_>>();
    // Stable sort keeps the sidebar order inside each group.
    rows.sort_by_key(|row| row.agent.agent_status != AgentStatus::Blocked);
    rows.into_iter()
        .map(|row| super::aggregate_navigation::AggregateAgentTarget {
            endpoint_id: row.endpoint.endpoint_id.clone(),
            pane_id: row.agent.pane_id.clone(),
        })
        .collect()
}

impl ClientShellState {
    /// The waiting agent after `current`, wrapping around; the first one when
    /// `current` is not waiting.
    fn next_waiting_agent_target(
        &self,
        current: Option<(&ClientEndpointId, &str)>,
    ) -> Option<super::aggregate_navigation::AggregateAgentTarget> {
        let mut targets = waiting_agent_targets(
            &self.endpoints,
            &self.active_endpoint_id,
            self.config.agent_panel_sort,
        );
        if targets.is_empty() {
            return None;
        }
        let next = current
            .and_then(|(endpoint_id, pane_id)| {
                targets.iter().position(|target| {
                    &target.endpoint_id == endpoint_id && target.pane_id == pane_id
                })
            })
            .map_or(0, |index| (index + 1) % targets.len());
        Some(targets.swap_remove(next))
    }

    /// `prefix+i`: focus the next agent that waits for the user. Shows a short
    /// hint when no agent is blocked or has an unviewed completion.
    pub(super) fn focus_next_waiting_agent(&mut self, outcome: &mut ClientShellInput) {
        let focused_pane = self
            .snapshot
            .as_deref()
            .and_then(|snapshot| snapshot.focused_pane_id.clone());
        let active = self.active_endpoint_id.clone();
        let current = focused_pane.as_deref().map(|pane_id| (&active, pane_id));
        let Some(target) = self.next_waiting_agent_target(current) else {
            self.show_no_waiting_agent(outcome);
            return;
        };
        self.focus_or_activate(
            target.endpoint_id,
            ClientEndpointFocusTarget::Pane(target.pane_id),
            outcome,
        );
        outcome.repaint = true;
    }

    /// `i` inside sidebar navigation: move the agent selection to the next
    /// waiting agent without changing focus; Enter opens it as usual.
    pub(super) fn select_next_waiting_agent(&mut self, outcome: &mut ClientShellInput) {
        if self.agent_navigation.is_none() {
            self.begin_agent_navigation(outcome);
        }
        let current = self.selected_navigation_agent().map(|selected| {
            (
                selected.workspace.endpoint_id.clone(),
                selected.pane_id.clone(),
            )
        });
        let Some(target) = self.next_waiting_agent_target(
            current
                .as_ref()
                .map(|(endpoint_id, pane_id)| (endpoint_id, pane_id.as_str())),
        ) else {
            self.show_no_waiting_agent(outcome);
            return;
        };
        let selected = self.agent_navigation_target(&target.endpoint_id, &target.pane_id);
        if selected.is_some() {
            self.agent_navigation = Some(AgentNavigation {
                selected,
                reveal: true,
            });
            outcome.repaint = true;
        }
    }

    fn show_no_waiting_agent(&mut self, outcome: &mut ClientShellInput) {
        self.show_feedback(
            NO_WAITING_AGENT_MESSAGE,
            std::time::Instant::now(),
            std::time::Duration::from_secs(1),
        );
        outcome.repaint = true;
    }
}
