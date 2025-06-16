/// Ratatui demonstration for CloudFormation describe-stack functionality
use chrono::{DateTime, Utc};
use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
        MouseEventKind,
    },
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState, Table, TableState,
        Wrap,
    },
};
use std::io;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct StackInfo {
    pub stack_name: String,
    pub stack_id: String,
    pub stack_status: String,
    pub creation_time: DateTime<Utc>,
    pub description: Option<String>,
    pub parameters: Vec<(String, String)>,
    pub tags: Vec<(String, String)>,
    pub capabilities: Vec<String>,
    pub outputs: Vec<StackOutput>,
}

#[derive(Debug, Clone)]
pub struct StackOutput {
    pub output_key: String,
    pub output_value: String,
    pub description: Option<String>,
    pub export_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StackEvent {
    pub timestamp: DateTime<Utc>,
    pub logical_resource_id: String,
    pub physical_resource_id: Option<String>,
    pub resource_type: String,
    pub resource_status: String,
    pub resource_status_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct StackResource {
    pub logical_resource_id: String,
    pub physical_resource_id: Option<String>,
    pub resource_type: String,
    pub resource_status: String,
    pub resource_status_reason: Option<String>,
    pub drift_status: Option<String>,
    pub last_updated_timestamp: Option<DateTime<Utc>>,
}

#[derive(Debug)]
pub struct App {
    stack_info: StackInfo,
    events: Vec<StackEvent>,
    resources: Vec<StackResource>,
    current_tab: usize,
    events_table_state: TableState,
    resources_table_state: TableState,
    // Real-time simulation
    last_event_time: Instant,
    simulation_stage: SimulationStage,
    pending_events: Vec<StackEvent>,
    update_counter: u32,
    // Sorting
    events_sort_column: EventsSortColumn,
    events_sort_ascending: bool,
    resources_sort_column: ResourcesSortColumn,
    resources_sort_ascending: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum EventsSortColumn {
    Time,
    ResourceId,
    ResourceType,
    Status,
    Reason,
}

#[derive(Debug, Clone, Copy)]
pub enum ResourcesSortColumn {
    LogicalId,
    ResourceType,
    Status,
    PhysicalId,
    DriftStatus,
}

#[derive(Debug, Clone)]
pub enum SimulationStage {
    Idle,
    UpdatingStack,
    AddingResource,
    DriftDetection,
    RollingBack,
}

impl App {
    pub fn new() -> App {
        let mut events_table_state = TableState::default();
        events_table_state.select(Some(0));
        let mut resources_table_state = TableState::default();
        resources_table_state.select(Some(0));

        App {
            stack_info: create_fake_stack_info(),
            events: create_fake_events(),
            resources: create_fake_resources(),
            current_tab: 0,
            events_table_state,
            resources_table_state,
            last_event_time: Instant::now(),
            simulation_stage: SimulationStage::Idle,
            pending_events: create_pending_events(),
            update_counter: 0,
            events_sort_column: EventsSortColumn::Time,
            events_sort_ascending: false, // Most recent first
            resources_sort_column: ResourcesSortColumn::LogicalId,
            resources_sort_ascending: true,
        }
    }

    pub fn next_tab(&mut self) {
        self.current_tab = (self.current_tab + 1) % 3;
    }

    pub fn previous_tab(&mut self) {
        if self.current_tab > 0 {
            self.current_tab -= 1;
        } else {
            self.current_tab = 2;
        }
    }

    pub fn scroll_down(&mut self) {
        match self.current_tab {
            1 => {
                let events_len = self.get_sorted_events().len();
                if let Some(selected) = self.events_table_state.selected() {
                    if selected < events_len.saturating_sub(1) {
                        self.events_table_state.select(Some(selected + 1));
                    }
                } else if events_len > 0 {
                    self.events_table_state.select(Some(0));
                }
            }
            2 => {
                let resources_len = self.get_sorted_resources().len();
                if let Some(selected) = self.resources_table_state.selected() {
                    if selected < resources_len.saturating_sub(1) {
                        self.resources_table_state.select(Some(selected + 1));
                    }
                } else if resources_len > 0 {
                    self.resources_table_state.select(Some(0));
                }
            }
            _ => {}
        }
    }

    pub fn scroll_up(&mut self) {
        match self.current_tab {
            1 => {
                if let Some(selected) = self.events_table_state.selected() {
                    if selected > 0 {
                        self.events_table_state.select(Some(selected - 1));
                    }
                } else {
                    let events_len = self.get_sorted_events().len();
                    if events_len > 0 {
                        self.events_table_state.select(Some(events_len - 1));
                    }
                }
            }
            2 => {
                if let Some(selected) = self.resources_table_state.selected() {
                    if selected > 0 {
                        self.resources_table_state.select(Some(selected - 1));
                    }
                } else {
                    let resources_len = self.get_sorted_resources().len();
                    if resources_len > 0 {
                        self.resources_table_state.select(Some(resources_len - 1));
                    }
                }
            }
            _ => {}
        }
    }

    pub fn sort_events(&mut self, column: EventsSortColumn) {
        if std::mem::discriminant(&self.events_sort_column) == std::mem::discriminant(&column) {
            self.events_sort_ascending = !self.events_sort_ascending;
        } else {
            self.events_sort_column = column;
            self.events_sort_ascending = true;
        }
        // Reset selection when sorting changes
        self.events_table_state.select(Some(0));
    }

    pub fn sort_resources(&mut self, column: ResourcesSortColumn) {
        if std::mem::discriminant(&self.resources_sort_column) == std::mem::discriminant(&column) {
            self.resources_sort_ascending = !self.resources_sort_ascending;
        } else {
            self.resources_sort_column = column;
            self.resources_sort_ascending = true;
        }
        // Reset selection when sorting changes
        self.resources_table_state.select(Some(0));
    }

    pub fn get_sorted_events(&self) -> Vec<&StackEvent> {
        let mut events: Vec<&StackEvent> = self.events.iter().collect();

        events.sort_by(|a, b| {
            let ordering = match self.events_sort_column {
                EventsSortColumn::Time => a.timestamp.cmp(&b.timestamp),
                EventsSortColumn::ResourceId => a.logical_resource_id.cmp(&b.logical_resource_id),
                EventsSortColumn::ResourceType => a.resource_type.cmp(&b.resource_type),
                EventsSortColumn::Status => a.resource_status.cmp(&b.resource_status),
                EventsSortColumn::Reason => {
                    let a_reason = a.resource_status_reason.as_deref().unwrap_or("");
                    let b_reason = b.resource_status_reason.as_deref().unwrap_or("");
                    a_reason.cmp(b_reason)
                }
            };

            if self.events_sort_ascending {
                ordering
            } else {
                ordering.reverse()
            }
        });

        events
    }

    pub fn handle_mouse_click(&mut self, x: u16, y: u16, full_area: Rect) {
        // Check if click is in tab bar area (first 3 lines)
        if y < 3 {
            self.handle_tab_click(x, y, full_area);
            return;
        }

        // Calculate content area (below tab bar)
        let content_y = 3;
        let content_area = Rect {
            x: full_area.x,
            y: content_y,
            width: full_area.width,
            height: full_area.height.saturating_sub(3),
        };

        match self.current_tab {
            1 => self.handle_events_mouse_click(x, y, content_area),
            2 => self.handle_resources_mouse_click(x, y, content_area),
            _ => {}
        }
    }

    fn handle_tab_click(&mut self, x: u16, _y: u16, area: Rect) {
        // Tab titles are roughly: "Stack Info", "Events", "Resources"
        // Each tab is approximately area.width / 3
        let tab_width = area.width / 3;

        if x < tab_width {
            self.current_tab = 0; // Stack Info
        } else if x < tab_width * 2 {
            self.current_tab = 1; // Events
        } else {
            self.current_tab = 2; // Resources
        }
    }

    fn handle_events_mouse_click(&mut self, x: u16, y: u16, area: Rect) {
        // Check if click is in header area for sorting
        if y == area.y + 1 {
            // Header is at y + 1 (after title)
            // Determine which column was clicked based on x position
            let mut col_start = area.x;
            let constraints = [10u16, 25, 25, 20]; // Time, Resource ID, Type, Status (Reason takes remaining)

            for (i, &width) in constraints.iter().enumerate() {
                if x >= col_start && x < col_start + width {
                    match i {
                        0 => self.sort_events(EventsSortColumn::Time),
                        1 => self.sort_events(EventsSortColumn::ResourceId),
                        2 => self.sort_events(EventsSortColumn::ResourceType),
                        3 => self.sort_events(EventsSortColumn::Status),
                        _ => {}
                    }
                    return;
                }
                col_start += width;
            }
            // If click is beyond the fixed columns, it's the Reason column
            if x >= col_start {
                self.sort_events(EventsSortColumn::Reason);
            }
        } else if y > area.y + 1 {
            // Click in data rows
            let row_index = (y - area.y - 2) as usize; // -2 for title and header
            let events = self.get_sorted_events();
            if row_index < events.len() {
                self.events_table_state.select(Some(row_index));
            }
        }
    }

    fn handle_resources_mouse_click(&mut self, x: u16, y: u16, area: Rect) {
        // Check if click is in header area for sorting
        if y == area.y + 1 {
            // Header is at y + 1 (after title)
            // Determine which column was clicked based on x position
            let mut col_start = area.x;
            let constraints = [25u16, 30, 18, 15]; // Logical ID, Type, Status, Drift (Physical ID takes remaining)

            for (i, &width) in constraints.iter().enumerate() {
                if x >= col_start && x < col_start + width {
                    match i {
                        0 => self.sort_resources(ResourcesSortColumn::LogicalId),
                        1 => self.sort_resources(ResourcesSortColumn::ResourceType),
                        2 => self.sort_resources(ResourcesSortColumn::Status),
                        3 => self.sort_resources(ResourcesSortColumn::DriftStatus),
                        _ => {}
                    }
                    return;
                }
                col_start += width;
            }
            // If click is beyond the fixed columns, it's the Physical ID column
            if x >= col_start {
                self.sort_resources(ResourcesSortColumn::PhysicalId);
            }
        } else if y > area.y + 1 {
            // Click in data rows
            let row_index = (y - area.y - 2) as usize; // -2 for title and header
            let resources = self.get_sorted_resources();
            if row_index < resources.len() {
                self.resources_table_state.select(Some(row_index));
            }
        }
    }

    pub fn get_column_header(
        &self,
        name: &str,
        column: &EventsSortColumn,
        current_sort: EventsSortColumn,
        ascending: bool,
    ) -> String {
        if std::mem::discriminant(column) == std::mem::discriminant(&current_sort) {
            if ascending {
                format!("{} ▲", name)
            } else {
                format!("{} ▼", name)
            }
        } else {
            name.to_string()
        }
    }

    pub fn get_resource_column_header(
        &self,
        name: &str,
        column: &ResourcesSortColumn,
        current_sort: ResourcesSortColumn,
        ascending: bool,
    ) -> String {
        if std::mem::discriminant(column) == std::mem::discriminant(&current_sort) {
            if ascending {
                format!("{} ▲", name)
            } else {
                format!("{} ▼", name)
            }
        } else {
            name.to_string()
        }
    }

    pub fn get_sorted_resources(&self) -> Vec<&StackResource> {
        let mut resources: Vec<&StackResource> = self.resources.iter().collect();

        resources.sort_by(|a, b| {
            let ordering = match self.resources_sort_column {
                ResourcesSortColumn::LogicalId => a.logical_resource_id.cmp(&b.logical_resource_id),
                ResourcesSortColumn::ResourceType => a.resource_type.cmp(&b.resource_type),
                ResourcesSortColumn::Status => a.resource_status.cmp(&b.resource_status),
                ResourcesSortColumn::PhysicalId => {
                    let a_physical = a.physical_resource_id.as_deref().unwrap_or("");
                    let b_physical = b.physical_resource_id.as_deref().unwrap_or("");
                    a_physical.cmp(b_physical)
                }
                ResourcesSortColumn::DriftStatus => {
                    let a_drift = a.drift_status.as_deref().unwrap_or("");
                    let b_drift = b.drift_status.as_deref().unwrap_or("");
                    a_drift.cmp(b_drift)
                }
            };

            if self.resources_sort_ascending {
                ordering
            } else {
                ordering.reverse()
            }
        });

        resources
    }

    /// Simulate real-time CloudFormation events (like watch-stack)
    pub fn update_simulation(&mut self) {
        let now = Instant::now();
        if now.duration_since(self.last_event_time) >= Duration::from_secs(3) {
            self.add_simulated_event();
            self.last_event_time = now;
            self.update_counter += 1;
        }
    }

    fn add_simulated_event(&mut self) {
        if self.pending_events.is_empty() {
            // Start a new simulation cycle
            self.pending_events = match self.simulation_stage {
                SimulationStage::Idle => {
                    self.simulation_stage = SimulationStage::UpdatingStack;
                    create_stack_update_events()
                }
                SimulationStage::UpdatingStack => {
                    self.simulation_stage = SimulationStage::AddingResource;
                    create_add_resource_events()
                }
                SimulationStage::AddingResource => {
                    self.simulation_stage = SimulationStage::DriftDetection;
                    create_drift_detection_events()
                }
                SimulationStage::DriftDetection => {
                    self.simulation_stage = SimulationStage::RollingBack;
                    create_rollback_events()
                }
                SimulationStage::RollingBack => {
                    self.simulation_stage = SimulationStage::Idle;
                    vec![]
                }
            };
        }

        if let Some(event) = self.pending_events.pop() {
            // Update timestamp to current time
            let mut new_event = event;
            new_event.timestamp = Utc::now();

            // Add to front of events list (most recent first)
            self.events.insert(0, new_event.clone());

            // Update corresponding resource status if it exists
            self.update_resource_status(&new_event);

            // Update stack status
            self.update_stack_status(&new_event);

            // Keep current selection stable - don't auto-reset to top

            // Keep only last 50 events to prevent memory growth
            if self.events.len() > 50 {
                self.events.truncate(50);
            }
        }
    }

    fn update_resource_status(&mut self, event: &StackEvent) {
        if let Some(resource) = self
            .resources
            .iter_mut()
            .find(|r| r.logical_resource_id == event.logical_resource_id)
        {
            resource.resource_status = event.resource_status.clone();
            resource.resource_status_reason = event.resource_status_reason.clone();
            resource.last_updated_timestamp = Some(event.timestamp);
        }
    }

    fn update_stack_status(&mut self, event: &StackEvent) {
        if event.logical_resource_id == self.stack_info.stack_name {
            self.stack_info.stack_status = event.resource_status.clone();
        }
    }
}

pub fn run_ratatui_demo() -> Result<(), Box<dyn std::error::Error>> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app and run
    let mut app = App::new();
    let res = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{err:?}");
    }

    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;

        // Check for events with timeout to allow periodic updates
        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        match key.code {
                            KeyCode::Char('q') => return Ok(()),
                            KeyCode::Tab => app.next_tab(),
                            KeyCode::BackTab => app.previous_tab(),
                            KeyCode::Down | KeyCode::Char('j') => app.scroll_down(),
                            KeyCode::Up | KeyCode::Char('k') => app.scroll_up(),
                            KeyCode::Char('r') => {
                                // Reset simulation
                                *app = App::new();
                            }
                            KeyCode::Char('1') => app.sort_events(EventsSortColumn::Time),
                            KeyCode::Char('2') => app.sort_events(EventsSortColumn::ResourceId),
                            KeyCode::Char('3') => app.sort_events(EventsSortColumn::ResourceType),
                            KeyCode::Char('4') => app.sort_events(EventsSortColumn::Status),
                            KeyCode::Char('5') => app.sort_events(EventsSortColumn::Reason),
                            KeyCode::Char('!') => {
                                app.sort_resources(ResourcesSortColumn::LogicalId)
                            }
                            KeyCode::Char('@') => {
                                app.sort_resources(ResourcesSortColumn::ResourceType)
                            }
                            KeyCode::Char('#') => app.sort_resources(ResourcesSortColumn::Status),
                            KeyCode::Char('$') => {
                                app.sort_resources(ResourcesSortColumn::PhysicalId)
                            }
                            KeyCode::Char('%') => {
                                app.sort_resources(ResourcesSortColumn::DriftStatus)
                            }
                            _ => {}
                        }
                    }
                }
                Event::Mouse(mouse) => {
                    if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                        let full_area = terminal.size()?;
                        app.handle_mouse_click(mouse.column, mouse.row, full_area);
                    }
                }
                _ => {}
            }
        }

        // Update simulation (add new events every 3 seconds)
        app.update_simulation();
    }
}

fn ui(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(f.size());

    // Tab bar
    let tabs = vec!["Stack Info", "Events", "Resources"];
    let tab_titles = tabs
        .iter()
        .enumerate()
        .map(|(i, &title)| {
            let style = if i == app.current_tab {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::UNDERLINED)
            };
            let text = if i == app.current_tab {
                format!("[{}]", title)
            } else {
                format!(" {} ", title)
            };
            Span::styled(text, style)
        })
        .collect::<Vec<_>>();

    let tabs_line = Line::from(tab_titles);
    let help_text = match app.current_tab {
        1 => format!(
            "Events - Sort: 1:Time 2:Resource 3:Type 4:Status 5:Reason or click headers | ↑↓/jk/mouse: navigate, tabs clickable, r: reset, q: quit | Updates: {} | Stage: {:?}",
            app.update_counter, app.simulation_stage
        ),
        2 => format!(
            "Resources - Sort: !:ID @:Type #:Status $:Physical %:Drift or click headers | ↑↓/jk/mouse: navigate, tabs clickable, r: reset, q: quit | Updates: {} | Stage: {:?}",
            app.update_counter, app.simulation_stage
        ),
        _ => format!(
            "CloudFormation Stack - Click tabs to switch, r: reset, q: quit | Updates: {} | Stage: {:?}",
            app.update_counter, app.simulation_stage
        ),
    };

    let tabs_paragraph = Paragraph::new(Text::from(vec![Line::from(help_text), tabs_line]))
        .block(Block::default().title("iidy watch-stack (Live Demo)"));

    f.render_widget(tabs_paragraph, chunks[0]);

    // Content area
    match app.current_tab {
        0 => render_stack_info(f, chunks[1], &app.stack_info),
        1 => render_events(f, chunks[1], app),
        2 => render_resources(f, chunks[1], app),
        _ => {}
    }
}

fn render_stack_info(f: &mut Frame, area: Rect, stack_info: &StackInfo) {
    // Split into left and right columns
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Left column
            Constraint::Percentage(50), // Right column
        ])
        .split(area);

    // Left column layout
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10), // Basic info
            Constraint::Min(0),     // Parameters
        ])
        .split(main_chunks[0]);

    // Right column layout
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6), // Tags
            Constraint::Min(0),    // Outputs
        ])
        .split(main_chunks[1]);

    // Basic stack information (left top)
    let basic_info = vec![
        format!("Name: {}", stack_info.stack_name),
        format!("Status: {}", colorize_status(&stack_info.stack_status)),
        format!(
            "Created: {}",
            stack_info.creation_time.format("%Y-%m-%d %H:%M:%S UTC")
        ),
        format!("Capabilities: {}", stack_info.capabilities.join(", ")),
        "".to_string(), // spacing
        format!("Description:"),
        format!(
            "  {}",
            stack_info
                .description
                .as_ref()
                .unwrap_or(&"None".to_string())
        ),
        "".to_string(), // spacing
        format!("Stack ID:"),
        format!("  {}", stack_info.stack_id),
    ];

    let basic_paragraph = Paragraph::new(basic_info.join("\n"))
        .block(Block::default().title("Stack Overview"))
        .wrap(Wrap { trim: true });
    f.render_widget(basic_paragraph, left_chunks[0]);

    // Parameters (left bottom)
    if !stack_info.parameters.is_empty() {
        let param_rows: Vec<Row> = stack_info
            .parameters
            .iter()
            .map(|(key, value)| Row::new(vec![key.clone(), value.clone()]))
            .collect();

        let params_table = Table::new(
            param_rows,
            [Constraint::Percentage(45), Constraint::Percentage(55)],
        )
        .header(
            Row::new(vec!["Parameter", "Value"])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(Block::default().title("Parameters"));
        f.render_widget(params_table, left_chunks[1]);
    }

    // Tags (right top)
    if !stack_info.tags.is_empty() {
        let tag_rows: Vec<Row> = stack_info
            .tags
            .iter()
            .map(|(key, value)| Row::new(vec![key.clone(), value.clone()]))
            .collect();

        let tags_table = Table::new(
            tag_rows,
            [Constraint::Percentage(45), Constraint::Percentage(55)],
        )
        .header(Row::new(vec!["Tag", "Value"]).style(Style::default().add_modifier(Modifier::BOLD)))
        .block(Block::default().title("Tags"));
        f.render_widget(tags_table, right_chunks[0]);
    }

    // Outputs (right bottom)
    if !stack_info.outputs.is_empty() {
        let output_rows: Vec<Row> = stack_info
            .outputs
            .iter()
            .map(|output| {
                Row::new(vec![
                    output.output_key.clone(),
                    output.output_value.clone(),
                    output
                        .description
                        .as_ref()
                        .unwrap_or(&"".to_string())
                        .clone(),
                    output
                        .export_name
                        .as_ref()
                        .unwrap_or(&"".to_string())
                        .clone(),
                ])
            })
            .collect();

        let outputs_table = Table::new(
            output_rows,
            [
                Constraint::Length(15),
                Constraint::Percentage(35),
                Constraint::Percentage(35),
                Constraint::Length(15),
            ],
        )
        .header(
            Row::new(vec!["Key", "Value", "Description", "Export"])
                .style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .block(Block::default().title("Outputs"));
        f.render_widget(outputs_table, right_chunks[1]);
    }
}

fn render_events(f: &mut Frame, area: Rect, app: &mut App) {
    let sorted_events = app.get_sorted_events();
    let events_count = sorted_events.len();
    let total_events = app.events.len();

    let event_rows: Vec<Row> = sorted_events
        .iter()
        .map(|event| {
            Row::new(vec![
                event.timestamp.format("%H:%M:%S").to_string(),
                event.logical_resource_id.clone(),
                event.resource_type.clone(),
                colorize_status(&event.resource_status),
                event
                    .resource_status_reason
                    .as_ref()
                    .unwrap_or(&"".to_string())
                    .clone(),
            ])
        })
        .collect();

    let events_table = Table::new(
        event_rows,
        [
            Constraint::Length(10), // Time
            Constraint::Length(25), // Resource ID
            Constraint::Length(25), // Resource Type
            Constraint::Length(20), // Status
            Constraint::Min(20),    // Reason
        ],
    )
    .header(
        Row::new(vec![
            app.get_column_header(
                "Time",
                &EventsSortColumn::Time,
                app.events_sort_column,
                app.events_sort_ascending,
            ),
            app.get_column_header(
                "Resource ID",
                &EventsSortColumn::ResourceId,
                app.events_sort_column,
                app.events_sort_ascending,
            ),
            app.get_column_header(
                "Type",
                &EventsSortColumn::ResourceType,
                app.events_sort_column,
                app.events_sort_ascending,
            ),
            app.get_column_header(
                "Status",
                &EventsSortColumn::Status,
                app.events_sort_column,
                app.events_sort_ascending,
            ),
            app.get_column_header(
                "Reason",
                &EventsSortColumn::Reason,
                app.events_sort_column,
                app.events_sort_ascending,
            ),
        ])
        .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(Block::default().title(format!("Stack Events ({}/{})", events_count, total_events)))
    .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
    .highlight_symbol(">> ");

    f.render_stateful_widget(events_table, area, &mut app.events_table_state);

    // Scrollbar
    let visible_height = area.height.saturating_sub(2) as usize; // Account for header
    if events_count > visible_height {
        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        let selected = app.events_table_state.selected().unwrap_or(0);
        let mut scrollbar_state = ScrollbarState::default()
            .content_length(events_count)
            .position(selected);

        f.render_stateful_widget(
            scrollbar,
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
}

fn render_resources(f: &mut Frame, area: Rect, app: &mut App) {
    let sorted_resources = app.get_sorted_resources();
    let resources_count = sorted_resources.len();
    let total_resources = app.resources.len();

    let resource_rows: Vec<Row> = sorted_resources
        .iter()
        .map(|resource| {
            Row::new(vec![
                resource.logical_resource_id.clone(),
                resource.resource_type.clone(),
                colorize_status(&resource.resource_status),
                resource
                    .physical_resource_id
                    .as_ref()
                    .unwrap_or(&"N/A".to_string())
                    .clone(),
                resource
                    .drift_status
                    .as_ref()
                    .unwrap_or(&"N/A".to_string())
                    .clone(),
            ])
        })
        .collect();

    let resources_table = Table::new(
        resource_rows,
        [
            Constraint::Length(25), // Logical ID
            Constraint::Length(30), // Resource Type
            Constraint::Length(18), // Status
            Constraint::Min(25),    // Physical ID
            Constraint::Length(15), // Drift Status
        ],
    )
    .header(
        Row::new(vec![
            app.get_resource_column_header(
                "Logical ID",
                &ResourcesSortColumn::LogicalId,
                app.resources_sort_column,
                app.resources_sort_ascending,
            ),
            app.get_resource_column_header(
                "Type",
                &ResourcesSortColumn::ResourceType,
                app.resources_sort_column,
                app.resources_sort_ascending,
            ),
            app.get_resource_column_header(
                "Status",
                &ResourcesSortColumn::Status,
                app.resources_sort_column,
                app.resources_sort_ascending,
            ),
            app.get_resource_column_header(
                "Physical ID",
                &ResourcesSortColumn::PhysicalId,
                app.resources_sort_column,
                app.resources_sort_ascending,
            ),
            app.get_resource_column_header(
                "Drift",
                &ResourcesSortColumn::DriftStatus,
                app.resources_sort_column,
                app.resources_sort_ascending,
            ),
        ])
        .style(Style::default().add_modifier(Modifier::BOLD)),
    )
    .block(Block::default().title(format!(
        "Stack Resources ({}/{})",
        resources_count, total_resources
    )))
    .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
    .highlight_symbol(">> ");

    f.render_stateful_widget(resources_table, area, &mut app.resources_table_state);

    // Scrollbar
    let visible_height = area.height.saturating_sub(2) as usize; // Account for header
    if resources_count > visible_height {
        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .begin_symbol(Some("↑"))
            .end_symbol(Some("↓"));

        let selected = app.resources_table_state.selected().unwrap_or(0);
        let mut scrollbar_state = ScrollbarState::default()
            .content_length(resources_count)
            .position(selected);

        f.render_stateful_widget(
            scrollbar,
            area.inner(Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut scrollbar_state,
        );
    }
}

fn colorize_status(status: &str) -> String {
    // In a real implementation, you'd use Style with colors
    // For now, we'll add emoji indicators
    match status {
        s if s.contains("COMPLETE") => format!("✅ {}", s),
        s if s.contains("FAILED") => format!("❌ {}", s),
        s if s.contains("PROGRESS") => format!("🔄 {}", s),
        s if s.contains("ROLLBACK") => format!("⏪ {}", s),
        _ => status.to_string(),
    }
}

// Fake data generation functions
fn create_pending_events() -> Vec<StackEvent> {
    vec![]
}

fn create_stack_update_events() -> Vec<StackEvent> {
    vec![
        StackEvent {
            timestamp: Utc::now(),
            logical_resource_id: "my-web-application".to_string(),
            physical_resource_id: None,
            resource_type: "AWS::CloudFormation::Stack".to_string(),
            resource_status: "UPDATE_IN_PROGRESS".to_string(),
            resource_status_reason: Some("User Initiated".to_string()),
        },
        StackEvent {
            timestamp: Utc::now(),
            logical_resource_id: "WebServerInstance".to_string(),
            physical_resource_id: Some("i-0123456789abcdef0".to_string()),
            resource_type: "AWS::EC2::Instance".to_string(),
            resource_status: "UPDATE_IN_PROGRESS".to_string(),
            resource_status_reason: Some("Updating instance type".to_string()),
        },
        StackEvent {
            timestamp: Utc::now(),
            logical_resource_id: "WebServerInstance".to_string(),
            physical_resource_id: Some("i-0123456789abcdef0".to_string()),
            resource_type: "AWS::EC2::Instance".to_string(),
            resource_status: "UPDATE_COMPLETE".to_string(),
            resource_status_reason: None,
        },
        StackEvent {
            timestamp: Utc::now(),
            logical_resource_id: "my-web-application".to_string(),
            physical_resource_id: None,
            resource_type: "AWS::CloudFormation::Stack".to_string(),
            resource_status: "UPDATE_COMPLETE".to_string(),
            resource_status_reason: None,
        },
    ]
}

fn create_add_resource_events() -> Vec<StackEvent> {
    vec![
        StackEvent {
            timestamp: Utc::now(),
            logical_resource_id: "my-web-application".to_string(),
            physical_resource_id: None,
            resource_type: "AWS::CloudFormation::Stack".to_string(),
            resource_status: "UPDATE_IN_PROGRESS".to_string(),
            resource_status_reason: Some("Adding new CloudWatch dashboard".to_string()),
        },
        StackEvent {
            timestamp: Utc::now(),
            logical_resource_id: "MonitoringDashboard".to_string(),
            physical_resource_id: Some("MyApp-Dashboard".to_string()),
            resource_type: "AWS::CloudWatch::Dashboard".to_string(),
            resource_status: "CREATE_IN_PROGRESS".to_string(),
            resource_status_reason: None,
        },
        StackEvent {
            timestamp: Utc::now(),
            logical_resource_id: "MonitoringDashboard".to_string(),
            physical_resource_id: Some("MyApp-Dashboard".to_string()),
            resource_type: "AWS::CloudWatch::Dashboard".to_string(),
            resource_status: "CREATE_COMPLETE".to_string(),
            resource_status_reason: None,
        },
        StackEvent {
            timestamp: Utc::now(),
            logical_resource_id: "my-web-application".to_string(),
            physical_resource_id: None,
            resource_type: "AWS::CloudFormation::Stack".to_string(),
            resource_status: "UPDATE_COMPLETE".to_string(),
            resource_status_reason: None,
        },
    ]
}

fn create_drift_detection_events() -> Vec<StackEvent> {
    vec![
        StackEvent {
            timestamp: Utc::now(),
            logical_resource_id: "LoadBalancerSecurityGroup".to_string(),
            physical_resource_id: Some("sg-0abcdef123456789".to_string()),
            resource_type: "AWS::EC2::SecurityGroup".to_string(),
            resource_status: "UPDATE_IN_PROGRESS".to_string(),
            resource_status_reason: Some("Drift detected - fixing configuration".to_string()),
        },
        StackEvent {
            timestamp: Utc::now(),
            logical_resource_id: "LoadBalancerSecurityGroup".to_string(),
            physical_resource_id: Some("sg-0abcdef123456789".to_string()),
            resource_type: "AWS::EC2::SecurityGroup".to_string(),
            resource_status: "UPDATE_COMPLETE".to_string(),
            resource_status_reason: Some("Drift corrected".to_string()),
        },
    ]
}

fn create_rollback_events() -> Vec<StackEvent> {
    vec![
        StackEvent {
            timestamp: Utc::now(),
            logical_resource_id: "my-web-application".to_string(),
            physical_resource_id: None,
            resource_type: "AWS::CloudFormation::Stack".to_string(),
            resource_status: "UPDATE_ROLLBACK_IN_PROGRESS".to_string(),
            resource_status_reason: Some("Failed to create new ALB listener - rolling back".to_string()),
        },
        StackEvent {
            timestamp: Utc::now(),
            logical_resource_id: "LoadBalancerListener".to_string(),
            physical_resource_id: Some("arn:aws:elasticloadbalancing:us-east-1:123456789012:listener/app/my-lb/50dc6c495c0c9188/f2f7dc8efc522ab2".to_string()),
            resource_type: "AWS::ElasticLoadBalancingV2::Listener".to_string(),
            resource_status: "DELETE_IN_PROGRESS".to_string(),
            resource_status_reason: Some("Resource creation cancelled".to_string()),
        },
        StackEvent {
            timestamp: Utc::now(),
            logical_resource_id: "LoadBalancerListener".to_string(),
            physical_resource_id: Some("arn:aws:elasticloadbalancing:us-east-1:123456789012:listener/app/my-lb/50dc6c495c0c9188/f2f7dc8efc522ab2".to_string()),
            resource_type: "AWS::ElasticLoadBalancingV2::Listener".to_string(),
            resource_status: "DELETE_COMPLETE".to_string(),
            resource_status_reason: None,
        },
        StackEvent {
            timestamp: Utc::now(),
            logical_resource_id: "my-web-application".to_string(),
            physical_resource_id: None,
            resource_type: "AWS::CloudFormation::Stack".to_string(),
            resource_status: "UPDATE_ROLLBACK_COMPLETE".to_string(),
            resource_status_reason: None,
        },
    ]
}

fn create_fake_stack_info() -> StackInfo {
    StackInfo {
        stack_name: "my-web-application".to_string(),
        stack_id: "arn:aws:cloudformation:us-east-1:123456789012:stack/my-web-application/12345678-1234-1234-1234-123456789012".to_string(),
        stack_status: "CREATE_COMPLETE".to_string(),
        creation_time: Utc::now() - chrono::Duration::hours(2),
        description: Some("Web application infrastructure with load balancer, EC2 instances, and RDS database".to_string()),
        parameters: vec![
            ("Environment".to_string(), "production".to_string()),
            ("InstanceType".to_string(), "t3.medium".to_string()),
            ("DBPassword".to_string(), "****".to_string()),
            ("KeyPairName".to_string(), "my-keypair".to_string()),
        ],
        tags: vec![
            ("Environment".to_string(), "production".to_string()),
            ("Team".to_string(), "DevOps".to_string()),
            ("Project".to_string(), "WebApp".to_string()),
        ],
        capabilities: vec!["CAPABILITY_IAM".to_string()],
        outputs: vec![
            StackOutput {
                output_key: "LoadBalancerDNS".to_string(),
                output_value: "my-lb-123456789.us-east-1.elb.amazonaws.com".to_string(),
                description: Some("DNS name of the load balancer".to_string()),
                export_name: Some("MyApp-LoadBalancer-DNS".to_string()),
            },
            StackOutput {
                output_key: "DatabaseEndpoint".to_string(),
                output_value: "mydb.xyz123.us-east-1.rds.amazonaws.com".to_string(),
                description: Some("RDS database endpoint".to_string()),
                export_name: None,
            },
        ],
    }
}

fn create_fake_events() -> Vec<StackEvent> {
    let base_time = Utc::now() - chrono::Duration::hours(2);
    vec![
        StackEvent {
            timestamp: base_time,
            logical_resource_id: "my-web-application".to_string(),
            physical_resource_id: None,
            resource_type: "AWS::CloudFormation::Stack".to_string(),
            resource_status: "CREATE_IN_PROGRESS".to_string(),
            resource_status_reason: Some("User Initiated".to_string()),
        },
        StackEvent {
            timestamp: base_time + chrono::Duration::seconds(30),
            logical_resource_id: "VPC".to_string(),
            physical_resource_id: Some("vpc-12345678".to_string()),
            resource_type: "AWS::EC2::VPC".to_string(),
            resource_status: "CREATE_IN_PROGRESS".to_string(),
            resource_status_reason: None,
        },
        StackEvent {
            timestamp: base_time + chrono::Duration::seconds(45),
            logical_resource_id: "VPC".to_string(),
            physical_resource_id: Some("vpc-12345678".to_string()),
            resource_type: "AWS::EC2::VPC".to_string(),
            resource_status: "CREATE_COMPLETE".to_string(),
            resource_status_reason: None,
        },
        StackEvent {
            timestamp: base_time + chrono::Duration::minutes(1),
            logical_resource_id: "PublicSubnet".to_string(),
            physical_resource_id: Some("subnet-87654321".to_string()),
            resource_type: "AWS::EC2::Subnet".to_string(),
            resource_status: "CREATE_IN_PROGRESS".to_string(),
            resource_status_reason: None,
        },
        StackEvent {
            timestamp: base_time + chrono::Duration::minutes(1) + chrono::Duration::seconds(20),
            logical_resource_id: "PublicSubnet".to_string(),
            physical_resource_id: Some("subnet-87654321".to_string()),
            resource_type: "AWS::EC2::Subnet".to_string(),
            resource_status: "CREATE_COMPLETE".to_string(),
            resource_status_reason: None,
        },
        StackEvent {
            timestamp: base_time + chrono::Duration::minutes(2),
            logical_resource_id: "LoadBalancer".to_string(),
            physical_resource_id: Some("my-lb-123456789".to_string()),
            resource_type: "AWS::ElasticLoadBalancingV2::LoadBalancer".to_string(),
            resource_status: "CREATE_IN_PROGRESS".to_string(),
            resource_status_reason: None,
        },
        StackEvent {
            timestamp: base_time + chrono::Duration::minutes(5),
            logical_resource_id: "LoadBalancer".to_string(),
            physical_resource_id: Some("my-lb-123456789".to_string()),
            resource_type: "AWS::ElasticLoadBalancingV2::LoadBalancer".to_string(),
            resource_status: "CREATE_COMPLETE".to_string(),
            resource_status_reason: None,
        },
        StackEvent {
            timestamp: base_time + chrono::Duration::minutes(6),
            logical_resource_id: "WebServerInstance".to_string(),
            physical_resource_id: Some("i-0123456789abcdef0".to_string()),
            resource_type: "AWS::EC2::Instance".to_string(),
            resource_status: "CREATE_IN_PROGRESS".to_string(),
            resource_status_reason: None,
        },
        StackEvent {
            timestamp: base_time + chrono::Duration::minutes(8),
            logical_resource_id: "WebServerInstance".to_string(),
            physical_resource_id: Some("i-0123456789abcdef0".to_string()),
            resource_type: "AWS::EC2::Instance".to_string(),
            resource_status: "CREATE_COMPLETE".to_string(),
            resource_status_reason: None,
        },
        StackEvent {
            timestamp: base_time + chrono::Duration::minutes(9),
            logical_resource_id: "DatabaseInstance".to_string(),
            physical_resource_id: Some("mydb-instance".to_string()),
            resource_type: "AWS::RDS::DBInstance".to_string(),
            resource_status: "CREATE_IN_PROGRESS".to_string(),
            resource_status_reason: None,
        },
        StackEvent {
            timestamp: base_time + chrono::Duration::minutes(15),
            logical_resource_id: "DatabaseInstance".to_string(),
            physical_resource_id: Some("mydb-instance".to_string()),
            resource_type: "AWS::RDS::DBInstance".to_string(),
            resource_status: "CREATE_COMPLETE".to_string(),
            resource_status_reason: None,
        },
        StackEvent {
            timestamp: base_time + chrono::Duration::minutes(16),
            logical_resource_id: "my-web-application".to_string(),
            physical_resource_id: None,
            resource_type: "AWS::CloudFormation::Stack".to_string(),
            resource_status: "CREATE_COMPLETE".to_string(),
            resource_status_reason: None,
        },
    ]
}

fn create_fake_resources() -> Vec<StackResource> {
    vec![
        StackResource {
            logical_resource_id: "VPC".to_string(),
            physical_resource_id: Some("vpc-12345678".to_string()),
            resource_type: "AWS::EC2::VPC".to_string(),
            resource_status: "CREATE_COMPLETE".to_string(),
            resource_status_reason: None,
            drift_status: Some("IN_SYNC".to_string()),
            last_updated_timestamp: Some(Utc::now() - chrono::Duration::hours(2)),
        },
        StackResource {
            logical_resource_id: "PublicSubnet".to_string(),
            physical_resource_id: Some("subnet-87654321".to_string()),
            resource_type: "AWS::EC2::Subnet".to_string(),
            resource_status: "CREATE_COMPLETE".to_string(),
            resource_status_reason: None,
            drift_status: Some("IN_SYNC".to_string()),
            last_updated_timestamp: Some(Utc::now() - chrono::Duration::hours(2)),
        },
        StackResource {
            logical_resource_id: "PrivateSubnet".to_string(),
            physical_resource_id: Some("subnet-13579246".to_string()),
            resource_type: "AWS::EC2::Subnet".to_string(),
            resource_status: "CREATE_COMPLETE".to_string(),
            resource_status_reason: None,
            drift_status: Some("IN_SYNC".to_string()),
            last_updated_timestamp: Some(Utc::now() - chrono::Duration::hours(2)),
        },
        StackResource {
            logical_resource_id: "InternetGateway".to_string(),
            physical_resource_id: Some("igw-98765432".to_string()),
            resource_type: "AWS::EC2::InternetGateway".to_string(),
            resource_status: "CREATE_COMPLETE".to_string(),
            resource_status_reason: None,
            drift_status: Some("IN_SYNC".to_string()),
            last_updated_timestamp: Some(Utc::now() - chrono::Duration::hours(2)),
        },
        StackResource {
            logical_resource_id: "LoadBalancer".to_string(),
            physical_resource_id: Some("my-lb-123456789".to_string()),
            resource_type: "AWS::ElasticLoadBalancingV2::LoadBalancer".to_string(),
            resource_status: "CREATE_COMPLETE".to_string(),
            resource_status_reason: None,
            drift_status: Some("IN_SYNC".to_string()),
            last_updated_timestamp: Some(Utc::now() - chrono::Duration::hours(2)),
        },
        StackResource {
            logical_resource_id: "LoadBalancerSecurityGroup".to_string(),
            physical_resource_id: Some("sg-0abcdef123456789".to_string()),
            resource_type: "AWS::EC2::SecurityGroup".to_string(),
            resource_status: "CREATE_COMPLETE".to_string(),
            resource_status_reason: None,
            drift_status: Some("DRIFTED".to_string()),
            last_updated_timestamp: Some(Utc::now() - chrono::Duration::hours(1)),
        },
        StackResource {
            logical_resource_id: "WebServerInstance".to_string(),
            physical_resource_id: Some("i-0123456789abcdef0".to_string()),
            resource_type: "AWS::EC2::Instance".to_string(),
            resource_status: "CREATE_COMPLETE".to_string(),
            resource_status_reason: None,
            drift_status: Some("IN_SYNC".to_string()),
            last_updated_timestamp: Some(Utc::now() - chrono::Duration::hours(2)),
        },
        StackResource {
            logical_resource_id: "WebServerSecurityGroup".to_string(),
            physical_resource_id: Some("sg-9876543210fedcba".to_string()),
            resource_type: "AWS::EC2::SecurityGroup".to_string(),
            resource_status: "CREATE_COMPLETE".to_string(),
            resource_status_reason: None,
            drift_status: Some("IN_SYNC".to_string()),
            last_updated_timestamp: Some(Utc::now() - chrono::Duration::hours(2)),
        },
        StackResource {
            logical_resource_id: "DatabaseInstance".to_string(),
            physical_resource_id: Some("mydb-instance".to_string()),
            resource_type: "AWS::RDS::DBInstance".to_string(),
            resource_status: "CREATE_COMPLETE".to_string(),
            resource_status_reason: None,
            drift_status: Some("IN_SYNC".to_string()),
            last_updated_timestamp: Some(Utc::now() - chrono::Duration::hours(2)),
        },
        StackResource {
            logical_resource_id: "DatabaseSubnetGroup".to_string(),
            physical_resource_id: Some("mydb-subnet-group".to_string()),
            resource_type: "AWS::RDS::DBSubnetGroup".to_string(),
            resource_status: "CREATE_COMPLETE".to_string(),
            resource_status_reason: None,
            drift_status: Some("IN_SYNC".to_string()),
            last_updated_timestamp: Some(Utc::now() - chrono::Duration::hours(2)),
        },
        StackResource {
            logical_resource_id: "IAMRole".to_string(),
            physical_resource_id: Some("MyWebApp-Role-ABC123DEF456".to_string()),
            resource_type: "AWS::IAM::Role".to_string(),
            resource_status: "CREATE_COMPLETE".to_string(),
            resource_status_reason: None,
            drift_status: Some("IN_SYNC".to_string()),
            last_updated_timestamp: Some(Utc::now() - chrono::Duration::hours(2)),
        },
    ]
}
