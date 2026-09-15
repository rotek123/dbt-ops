use crate::api::{Run};

pub struct App {
    pub run: Option<Run>,
    pub selected_step_idx: usize,
    pub scroll_offset: u16,
    pub auto_follow: bool,
    pub should_quit: bool,
    pub force_refresh: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            run: None,
            selected_step_idx: 0,
            scroll_offset: 0,
            auto_follow: true,
            should_quit: false,
            force_refresh: false,
        }
    }

    pub fn update_run(&mut self, new_run: Run) {
        let first_load = self.run.is_none();
        self.run = Some(new_run.clone());
        
        if first_load {
            if let Some(steps) = &new_run.run_steps {
                for (i, step) in steps.iter().enumerate() {
                    if step.status == 3 || step.status == 1 { // Running or Queued
                        self.selected_step_idx = i;
                        break;
                    }
                }
            }
        }
    }

    pub fn next_step(&mut self) {
        if let Some(run) = &self.run {
            if let Some(steps) = &run.run_steps {
                if !steps.is_empty() {
                    self.selected_step_idx = (self.selected_step_idx + 1) % steps.len();
                    self.reset_scroll();
                }
            }
        }
    }

    pub fn prev_step(&mut self) {
        if let Some(run) = &self.run {
            if let Some(steps) = &run.run_steps {
                if !steps.is_empty() {
                    if self.selected_step_idx == 0 {
                        self.selected_step_idx = steps.len() - 1;
                    } else {
                        self.selected_step_idx -= 1;
                    }
                    self.reset_scroll();
                }
            }
        }
    }

    pub fn scroll_down(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_add(1);
    }

    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
        self.auto_follow = false;
    }

    pub fn jump_to_bottom(&mut self) {
        self.auto_follow = true;
    }

    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
        self.auto_follow = true;
    }
}
