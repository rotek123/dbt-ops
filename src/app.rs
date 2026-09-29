use crate::api::Run;

#[derive(PartialEq)]
pub enum LogMode {
    Info,
    Debug,
}

#[derive(PartialEq)]
pub enum AppState {
    Dashboard,
    RunView,
}

pub struct App {
    pub state: AppState,
    pub runs: Vec<Run>,
    pub dashboard_selected_idx: usize,
    pub run: Option<Run>,
    pub selected_step_idx: usize,
    pub scroll_offset: u16,
    pub h_scroll_offset: u16,
    pub auto_follow: bool,
    pub should_quit: bool,
    pub log_mode: LogMode,
}

impl App {
    pub fn new() -> Self {
        Self {
            state: AppState::Dashboard,
            runs: Vec::new(),
            dashboard_selected_idx: 0,
            run: None,
            selected_step_idx: 0,
            scroll_offset: 0,
            h_scroll_offset: 0,
            auto_follow: true,
            should_quit: false,
            log_mode: LogMode::Info,
        }
    }

    pub fn update_run(&mut self, new_run: Run) {
        let first_load = self.run.is_none();
        self.run = Some(new_run.clone());

        if first_load && let Some(steps) = &new_run.run_steps {
            for (i, step) in steps.iter().enumerate() {
                if step.status == 3 || step.status == 1 {
                    // Running or Queued
                    self.selected_step_idx = i;
                    break;
                }
            }
        }
    }

    pub fn next_step(&mut self) {
        if let Some(run) = &self.run
            && let Some(steps) = &run.run_steps
            && !steps.is_empty()
        {
            self.selected_step_idx = (self.selected_step_idx + 1) % steps.len();
            self.reset_scroll();
        }
    }

    pub fn prev_step(&mut self) {
        if let Some(run) = &self.run
            && let Some(steps) = &run.run_steps
            && !steps.is_empty()
        {
            if self.selected_step_idx == 0 {
                self.selected_step_idx = steps.len() - 1;
            } else {
                self.selected_step_idx -= 1;
            }
            self.reset_scroll();
        }
    }

    pub fn scroll_down(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_add(amount);
    }

    pub fn scroll_up(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
        self.auto_follow = false;
    }

    pub fn scroll_right(&mut self) {
        self.h_scroll_offset = self.h_scroll_offset.saturating_add(2);
    }

    pub fn scroll_left(&mut self) {
        self.h_scroll_offset = self.h_scroll_offset.saturating_sub(2);
    }

    pub fn jump_to_top(&mut self) {
        self.scroll_offset = 0;
        self.auto_follow = false;
    }

    pub fn jump_to_bottom(&mut self) {
        self.auto_follow = true;
    }

    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
        self.h_scroll_offset = 0;
        self.auto_follow = true;
    }

    pub fn toggle_log_mode(&mut self) {
        if self.log_mode == LogMode::Info {
            self.log_mode = LogMode::Debug;
        } else {
            self.log_mode = LogMode::Info;
        }
    }

    pub fn dashboard_next(&mut self) {
        if !self.runs.is_empty() {
            self.dashboard_selected_idx =
                (self.dashboard_selected_idx + 1).min(self.runs.len() - 1);
        }
    }

    pub fn dashboard_prev(&mut self) {
        self.dashboard_selected_idx = self.dashboard_selected_idx.saturating_sub(1);
    }
}
