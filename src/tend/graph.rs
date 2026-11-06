//! Graf adapter: identity-safe topology, static positions, viewport and pointer mechanics.
//! Tend owns selection policy and linking; this starts no physics worker and writes nothing.

use super::Tend;
use crate::Palette;
use crate::clipped;
use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
use graf::{
    FeatureFlags, GraphAction, GraphMouseState, GraphState, LabelMode, NodeColorMode, NodeFill,
    NodeScale, NodeSpec, Settings, ThemeColors, apply_action, draw_graph_view, handle_graph_mouse,
    settings::SelectionFocus,
};
use parking_lot::RwLock;
use ratatui::{
    Frame,
    layout::{Position, Rect},
    style::{Color, Modifier},
    widgets::Paragraph,
};
use std::{collections::HashMap, sync::Arc};
use unicode_width::UnicodeWidthStr;

type Topology = (Vec<(String, String)>, Vec<(usize, usize)>);

#[derive(Default)]
pub(super) struct GraphView {
    topology: Topology,
    pending: Option<Topology>,
    appearance: Option<(Color, Color, String)>,
    pub(super) state: Option<Arc<RwLock<GraphState>>>,
    mouse: GraphMouseState,
    area: Rect,
    followed: Option<String>,
    pub(super) error: Option<String>,
    label_hits: Vec<(Rect, String)>,
    pointer_offset: Option<(i32, i32)>,
}

fn settings() -> Settings {
    let mut settings = Settings {
        max_node: 0,
        ..Settings::default()
    };
    settings.filter.show_orphan = true;
    settings.visual.node_color_mode = NodeColorMode::Uniform;
    settings.visual.node_fill = NodeFill::None;
    settings.visual.node_scale = NodeScale::Fixed(10);
    settings.visual.selection_focus = SelectionFocus::None;
    settings.visual.label_mode = LabelMode::None;
    settings.visual.show_legend = false;
    settings.visual.show_minimap = false;
    settings.visual.show_looking_glass = false;
    settings.visual.full_detail = true;
    settings.interaction.double_click_ms = 0;
    settings
}

fn identity(session: &super::Session) -> String {
    // Debug escaping preserves non-UTF8 path distinctions too.
    format!("{:?}", session.path)
}

impl Tend {
    /// Prepare path-bound inputs when sessions, scope or needs links change.
    pub(super) fn prepare_graph(&mut self) {
        let mut counts = HashMap::new();
        for session in &self.all_sessions {
            if let Some(id) = &session.id {
                *counts.entry(id.as_str()).or_insert(0) += 1;
            }
        }
        let mut nodes: Vec<_> = self
            .sessions
            .iter()
            .map(|s| (identity(s), s.goal.lines().next().unwrap_or("").to_owned()))
            .collect();
        nodes.sort_by(|a, b| a.0.cmp(&b.0));
        let indices: HashMap<_, _> = nodes
            .iter()
            .enumerate()
            .map(|(i, (id, _))| (id.as_str(), i))
            .collect();
        let ids: HashMap<_, _> = self
            .sessions
            .iter()
            .filter_map(|s| {
                let id = s.id.as_deref()?;
                (counts.get(id) == Some(&1)).then(|| (id, indices[identity(s).as_str()]))
            })
            .collect();
        let edges = self
            .links
            .as_ref()
            .map(|links| {
                links
                    .pairs()
                    .filter_map(|(a, b)| Some((*ids.get(a)?, *ids.get(b)?)))
                    .collect()
            })
            .unwrap_or_default();
        self.graph.pending = Some((nodes, edges));
    }

    pub(super) fn sync_graph(&mut self, body: Rect) {
        self.graph.sync(Rect::new(
            body.x,
            body.y,
            body.width,
            body.height.saturating_sub(3),
        ));
    }

    pub(crate) fn step_selection(&mut self, forward: bool) {
        self.selected = if forward {
            (self.selected + 1).min(self.sessions.len().saturating_sub(1))
        } else {
            self.selected.saturating_sub(1)
        };
        self.graph.followed = None;
    }

    pub(crate) fn adjust_view(&mut self, key: char) {
        if let Some(state) = &self.graph.state {
            let direction = match key {
                'K' => Some((0.0, 1.0)),
                'J' => Some((0.0, -1.0)),
                'H' => Some((-1.0, 0.0)),
                'L' => Some((1.0, 0.0)),
                _ => None,
            };
            if let Some((x, y)) = direction {
                let mut state = state.write();
                let step = 20.0 / state.viewport.zoom;
                state.viewport.pan_by(x * step, y * step);
            } else if let Some(action) = match key {
                '+' | '=' => Some(GraphAction::ZoomIn),
                '-' => Some(GraphAction::ZoomOut),
                _ => None,
            } {
                let _ = apply_action(state, action, &settings());
            }
        }
    }
}

impl GraphView {
    fn sync(&mut self, area: Rect) {
        let resized = self.area.width != area.width || self.area.height != area.height;
        self.area = area;
        let pending = self.pending.take();
        if self.state.is_some()
            && !resized
            && pending
                .as_ref()
                .is_none_or(|inputs| *inputs == self.topology)
        {
            return;
        }
        if let Some(inputs) = pending {
            self.topology = inputs;
        }
        let topology = &self.topology;
        let settings = settings();
        let specs: Vec<_> = topology
            .0
            .iter()
            .map(|(id, title)| NodeSpec {
                id: id.clone(),
                title: title.clone(),
                tags: vec![],
                folder: String::new(),
                links: vec![],
            })
            .collect();
        let mut state = match GraphState::from_specs(&specs, &settings) {
            Ok(state) => state,
            Err(error) => {
                self.error = Some(error.to_string());
                self.state = None;
                return;
            }
        };
        let indices: Vec<_> = state.simulation.get_graph().node_indices().collect();
        let graph = state.simulation.get_graph_mut();
        for &(a, b) in &topology.1 {
            graph.add_edge(indices[a], indices[b], ());
        }
        for &index in &indices {
            graph[index].data.link_count = graph.edges(index).count();
        }
        // Stable identity order in canvas-shaped slots leaves room for rings and short labels.
        // World units per row keep ring size steady while the number of slots grows.
        let width = f64::from(area.width.max(1));
        let height = f64::from(area.height.max(1));
        let count = indices.len().max(1);
        let rows = ((count as f64 * height * 2.4 / width).sqrt().round() as usize).clamp(1, count);
        let columns = count.div_ceil(rows);
        let world_per_row = settings.physics.ideal_distance / 6.0;
        let column_spacing = (width / columns as f64).max(12.0);
        let row_spacing = (height / rows as f64).max(6.0);
        for (slot, &index) in indices.iter().enumerate() {
            let node = &mut state.simulation.get_graph_mut()[index];
            node.location.x = ((slot % columns) as f64 + 0.5 - columns as f64 / 2.0) as f32
                * (column_spacing * world_per_row * graf::viewport::CELL_ASPECT) as f32;
            node.location.y = (rows as f64 / 2.0 - (slot / columns) as f64 - 0.5) as f32
                * (row_spacing * world_per_row) as f32;
            node.old_location = node.location;
        }
        state.graph_bounds = graf::render::compute_graph_bounds(state.simulation.get_graph());
        state.is_settled = true;
        state.alpha = 0.0;
        state.viewport.center_x = 0.0;
        state.viewport.center_y = 0.0;
        state.viewport.zoom = 100.0 / (height * world_per_row);
        state.viewport.auto_fit_zoom = state.viewport.zoom;
        if let Some(previous) = &self.state {
            let previous = previous.read();
            if resized {
                state.viewport.zoom *= previous.viewport.scale();
                state.viewport.center_x = previous.viewport.center_x;
                state.viewport.center_y = previous.viewport.center_y;
            } else {
                state.viewport = previous.viewport.clone();
            }
        }
        self.state = Some(Arc::new(RwLock::new(state)));
        self.error = None;
        self.followed = None;
        self.mouse = GraphMouseState::default();
        self.pointer_offset = None;
        self.label_hits.clear();
    }

    pub(super) fn pointer(&mut self, mut event: MouseEvent) -> Option<String> {
        if self.area.is_empty() {
            self.release();
            return None;
        }
        if !matches!(
            event.kind,
            MouseEventKind::Down(MouseButton::Left)
                | MouseEventKind::Up(MouseButton::Left)
                | MouseEventKind::Drag(MouseButton::Left)
                | MouseEventKind::ScrollUp
                | MouseEventKind::ScrollDown
        ) {
            return None;
        }
        let state = self.state.as_ref()?;
        let mut label_node = None;
        if matches!(event.kind, MouseEventKind::Down(MouseButton::Left)) {
            self.pointer_offset = None;
            if let Some((_, id)) = self
                .label_hits
                .iter()
                .find(|(rect, _)| rect.contains(Position::new(event.column, event.row)))
            {
                let state = state.read();
                let graph = state.simulation.get_graph();
                if let Some(index) = graph.node_indices().find(|&i| graph[i].data.id == *id) {
                    label_node = Some(index);
                    let node = &graph[index];
                    let (x, y) = state.viewport.world_to_screen(
                        f64::from(node.location.x),
                        f64::from(node.location.y),
                        self.area,
                    );
                    self.pointer_offset = Some((
                        x.round() as i32 - i32::from(event.column),
                        y.round() as i32 - i32::from(event.row),
                    ));
                }
            }
        }
        if matches!(
            event.kind,
            MouseEventKind::Down(MouseButton::Left)
                | MouseEventKind::Drag(MouseButton::Left)
                | MouseEventKind::Up(MouseButton::Left)
        ) && let Some((x, y)) = self.pointer_offset
        {
            event.column = (i32::from(event.column) + x).clamp(0, i32::from(u16::MAX)) as u16;
            event.row = (i32::from(event.row) + y).clamp(0, i32::from(u16::MAX)) as u16;
        }
        if matches!(event.kind, MouseEventKind::Up(MouseButton::Left)) {
            self.pointer_offset = None;
        }
        // Tend's identity-aware, injected clock owns double clicks, not graf's OpenFile.
        self.mouse.last_click_time = None;
        let _ = handle_graph_mouse(state, event, self.area, &mut self.mouse, &settings(), false);
        if let Some(index) = label_node {
            let mut state = state.write();
            state.selection.select_only(index);
            state.dragging_node = Some(index);
            self.mouse.is_panning = false;
        }
        if !matches!(event.kind, MouseEventKind::Down(MouseButton::Left))
            || !self.area.contains(Position::new(event.column, event.row))
        {
            return None;
        }
        let guard = state.read();
        let index = guard.selection.primary?;
        Some(guard.simulation.get_graph()[index].data.id.clone())
    }

    pub(super) fn release(&mut self) {
        self.mouse = GraphMouseState::default();
        self.pointer_offset = None;
        if let Some(state) = &self.state {
            let mut state = state.write();
            state.dragging_node = None;
            state.drag_target = None;
        }
    }

    pub(super) fn draw(
        &mut self,
        frame: &mut Frame,
        palette: Palette,
        selected: &str,
        source: &str,
        linking: bool,
    ) {
        let Some(shared) = self.state.clone() else {
            frame.render_widget(
                Paragraph::new("Goals couldn't be drawn. F1 then h for details; Esc goes back.")
                    .style(palette.muted),
                self.area,
            );
            return;
        };
        let mut state = shared.write();
        let graph = state.simulation.get_graph();
        let selected_index = graph.node_indices().find(|&i| graph[i].data.id == selected);
        let source_index = graph.node_indices().find(|&i| graph[i].data.id == source);
        if self.followed.as_deref() != Some(selected) {
            if let Some(index) = selected_index {
                let node = &graph[index];
                let (x, y) = state.viewport.world_to_screen(
                    f64::from(node.location.x),
                    f64::from(node.location.y),
                    self.area,
                );
                if x < f64::from(self.area.x + 3)
                    || x >= f64::from(self.area.right().saturating_sub(3))
                    || y < f64::from(self.area.y + 1)
                    || y >= f64::from(self.area.bottom().saturating_sub(1))
                {
                    let (x, y) = (node.location.x, node.location.y);
                    state.viewport.center_on_node(x, y);
                }
            }
            self.followed = Some(selected.to_owned());
        }
        state.selection.clear();
        if let Some(index) = source_index {
            state.selection.select_only(index);
        }
        let mut settings = settings();
        settings.visual.colors.node_color = Some(palette.ink.fg.unwrap_or(Color::Reset));
        settings.visual.colors.edge_color = Some(palette.muted.fg.unwrap_or(Color::Reset));
        settings.visual.colors.selection_ring_color = Some(palette.jade.fg.unwrap_or(Color::Reset));
        settings.visual.colors.background_color = Some(palette.ink.bg.unwrap_or(Color::Reset));
        let theme = ThemeColors::resolve(&settings);
        {
            let mut cache = state.render_cache.lock();
            let ink = palette.ink.fg.unwrap_or(Color::Reset);
            let jade = palette.jade.fg.unwrap_or(Color::Reset);
            let appearance_changed =
                self.appearance
                    .as_ref()
                    .is_none_or(|(old_ink, old_jade, old_source)| {
                        *old_ink != ink || *old_jade != jade || old_source != source
                    });
            if cache.topology_dirty || appearance_changed {
                cache.rebuild_topology(state.simulation.get_graph(), &settings, &theme, false);
                if let Some(index) = source_index {
                    cache.node_own_color.insert(index, jade);
                }
                self.appearance = Some((ink, jade, source.to_owned()));
            }
        }
        draw_graph_view(
            frame,
            self.area,
            &state,
            &settings,
            &theme,
            &FeatureFlags {
                show_legend: false,
                grid: false,
                show_minimap: false,
                show_status_bar: false,
            },
        );
        // Canvas shapes carry colour, not weight. Apply the selection's weight to its ring cells.
        let cache = state.render_cache.lock();
        if let Some(node) = cache.nodes.iter().find(|n| n.is_selected) {
            let aspect = f64::from(self.area.width) / f64::from(self.area.height.max(1));
            let xb = state.viewport.x_bounds(aspect);
            let yb = state.viewport.y_bounds(aspect);
            let project = |x: f64, y: f64| {
                (
                    f64::from(self.area.x)
                        + (x - xb[0]) / (xb[1] - xb[0])
                            * f64::from(self.area.width.saturating_sub(1)),
                    f64::from(self.area.y)
                        + (yb[1] - y) / (yb[1] - yb[0])
                            * f64::from(self.area.height.saturating_sub(1)),
                )
            };
            let r = node.radius + 1.5;
            let (left, top) = project(node.x - r, node.y + r);
            let (right, bottom) = project(node.x + r, node.y - r);
            for y in top.floor().max(f64::from(self.area.y)) as u16
                ..=bottom.ceil().min(f64::from(self.area.bottom() - 1)) as u16
            {
                for x in left.floor().max(f64::from(self.area.x)) as u16
                    ..=right.ceil().min(f64::from(self.area.right() - 1)) as u16
                {
                    frame.buffer_mut()[(x, y)].modifier.insert(Modifier::BOLD);
                }
            }
        }
        drop(cache);
        self.labels(frame, &state, palette, source, selected, linking);
    }

    fn labels(
        &mut self,
        frame: &mut Frame,
        state: &GraphState,
        palette: Palette,
        source: &str,
        selected: &str,
        linking: bool,
    ) {
        let graph = state.simulation.get_graph();
        let aspect = f64::from(self.area.width) / f64::from(self.area.height.max(1));
        let xb = state.viewport.x_bounds(aspect);
        let yb = state.viewport.y_bounds(aspect);
        let project = |x: f64, y: f64| {
            (
                f64::from(self.area.x)
                    + (x - xb[0]) / (xb[1] - xb[0]) * f64::from(self.area.width.saturating_sub(1)),
                f64::from(self.area.y)
                    + (yb[1] - y) / (yb[1] - yb[0]) * f64::from(self.area.height.saturating_sub(1)),
            )
        };
        let mut nodes: Vec<_> = graph
            .node_weights()
            .map(|node| {
                let (x, y) = project(f64::from(node.location.x), f64::from(node.location.y));
                (node, x.round() as i32, y.round() as i32)
            })
            .collect();
        nodes.sort_by_key(|(node, _, _)| {
            (
                node.data.id != selected,
                node.data.id != source,
                node.data.id.clone(),
            )
        });
        let mut occupied: Vec<Rect> = Vec::new();
        self.label_hits.clear();
        for (node, x, y) in nodes {
            if x < i32::from(self.area.x)
                || x >= i32::from(self.area.right())
                || y < i32::from(self.area.y)
                || y >= i32::from(self.area.bottom())
            {
                continue;
            }
            let marked = node.data.id == source;
            let prefix = if marked {
                "› "
            } else if linking && node.data.id == selected {
                "→ "
            } else {
                ""
            };
            let mut placed = false;
            for budget in [24, 18, 12, 8, 4] {
                let text = format!("{prefix}{}", clipped(&node.data.title, budget));
                let width = text.width().min(usize::from(self.area.width)) as u16;
                let left = (x - width as i32 / 2)
                    .clamp(i32::from(self.area.x), i32::from(self.area.right() - width))
                    as u16;
                for row in [y - 3, y + 2] {
                    let row = if node.data.id == selected {
                        row.clamp(i32::from(self.area.y), i32::from(self.area.bottom() - 1))
                    } else {
                        row
                    };
                    if row < i32::from(self.area.y) || row >= i32::from(self.area.bottom()) {
                        continue;
                    }
                    let rect = Rect::new(left, row as u16, width, 1);
                    if occupied
                        .iter()
                        .any(|other| !other.intersection(rect).is_empty())
                    {
                        continue;
                    }
                    frame.render_widget(
                        Paragraph::new(text.clone()).style(if marked {
                            palette.jade
                        } else {
                            palette.muted
                        }),
                        rect,
                    );
                    occupied.push(rect);
                    self.label_hits.push((rect, node.data.id.clone()));
                    placed = true;
                    break;
                }
                if placed {
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
