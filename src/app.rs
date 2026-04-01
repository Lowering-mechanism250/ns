use crate::net::{Connection, InterfaceStats, NetCollector, OpenPort};
use anyhow::Result;

#[derive(Debug, Clone, PartialEq)]
pub enum View {
    Load,
    Listeners,
    Outgoing,
    Incoming,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Modal {
    None,
    KillProcess {
        pid: u32,
        name: String,
        selected: usize,
    },
}

pub struct App {
    pub collector: NetCollector,
    pub view: View,
    pub modal: Modal,
    pub selected_iface_idx: usize,
    pub iface_scroll_offset: usize,
    pub list_scroll: usize,
    pub ports: Vec<OpenPort>,
    pub connections: Vec<Connection>,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Result<Self> {
        let mut collector = NetCollector::new();
        collector.refresh()?;
        Ok(Self {
            collector,
            view: View::Load,
            modal: Modal::None,
            selected_iface_idx: 0,
            iface_scroll_offset: 0,
            list_scroll: 0,
            ports: Vec::new(),
            connections: Vec::new(),
            should_quit: false,
        })
    }

    pub fn tick(&mut self) -> Result<()> {
        self.collector.refresh()?;
        let iface = self.selected_iface_name();
        self.ports = self.collector.get_open_ports(&iface);
        self.ports.sort_by_key(|p| p.port);
        self.connections = self.collector.get_connections(&iface);
        self.connections.sort_by(|a, b| {
            b.bytes_per_sec
                .partial_cmp(&a.bytes_per_sec)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(())
    }

    pub fn selected_iface_name(&self) -> String {
        let names = self.collector.interface_names();
        names
            .get(self.selected_iface_idx)
            .cloned()
            .unwrap_or_else(|| "all".to_string())
    }

    pub fn selected_interface(&self) -> Option<&InterfaceStats> {
        let name = self.selected_iface_name();
        self.collector.interfaces.get(&name)
    }

    pub fn iface_count(&self) -> usize {
        self.collector.interface_names().len()
    }

    pub fn next_iface(&mut self) {
        let count = self.iface_count();
        if count == 0 {
            return;
        }
        self.selected_iface_idx = (self.selected_iface_idx + 1) % count;
        self.list_scroll = 0;
        self.update_iface_scroll();
    }

    pub fn prev_iface(&mut self) {
        let count = self.iface_count();
        if count == 0 {
            return;
        }
        if self.selected_iface_idx == 0 {
            self.selected_iface_idx = count - 1;
        } else {
            self.selected_iface_idx -= 1;
        }
        self.list_scroll = 0;
        self.update_iface_scroll();
    }

    pub fn update_iface_scroll_for_width(&mut self, visible_count: usize) {
        if visible_count == 0 {
            return;
        }
        if self.selected_iface_idx < self.iface_scroll_offset {
            self.iface_scroll_offset = self.selected_iface_idx;
        } else if self.selected_iface_idx >= self.iface_scroll_offset + visible_count {
            self.iface_scroll_offset = self.selected_iface_idx + 1 - visible_count;
        }
    }

    fn update_iface_scroll(&mut self) {
        // Will be adjusted during render with actual visible_count
    }

    pub fn next_tab(&mut self) {
        self.view = match self.view {
            View::Load => View::Listeners,
            View::Listeners => View::Outgoing,
            View::Outgoing => View::Incoming,
            View::Incoming => View::Load,
        };
        self.list_scroll = 0;
    }

    /// Returns the number of items in the current list view.
    pub fn current_list_len(&self) -> usize {
        match self.view {
            View::Listeners => self.ports.len(),
            View::Outgoing => self.connections.iter().filter(|c| c.is_outgoing).count(),
            View::Incoming => self.connections.iter().filter(|c| !c.is_outgoing).count(),
            View::Load => 0,
        }
    }

    pub fn scroll_down(&mut self) {
        let max = self.current_list_len().saturating_sub(1);
        if self.list_scroll < max {
            self.list_scroll += 1;
        }
    }

    pub fn scroll_up(&mut self) {
        if self.list_scroll > 0 {
            self.list_scroll -= 1;
        }
    }

    pub fn enter_selected(&mut self) {
        if self.modal != Modal::None {
            return;
        }
        match self.view {
            View::Listeners => {
                if let Some(port) = self.ports.get(self.list_scroll) {
                    if let (Some(pid), Some(name)) = (port.pid, port.process_name.clone()) {
                        self.modal = Modal::KillProcess {
                            pid,
                            name,
                            selected: 0,
                        };
                    }
                }
            }
            View::Outgoing => {
                let outgoing: Vec<&Connection> =
                    self.connections.iter().filter(|c| c.is_outgoing).collect();
                if let Some(conn) = outgoing.get(self.list_scroll) {
                    if let (Some(pid), Some(name)) = (conn.pid, conn.process_name.clone()) {
                        self.modal = Modal::KillProcess {
                            pid,
                            name,
                            selected: 0,
                        };
                    }
                }
            }
            View::Incoming => {
                let incoming: Vec<&Connection> =
                    self.connections.iter().filter(|c| !c.is_outgoing).collect();
                if let Some(conn) = incoming.get(self.list_scroll) {
                    if let (Some(pid), Some(name)) = (conn.pid, conn.process_name.clone()) {
                        self.modal = Modal::KillProcess {
                            pid,
                            name,
                            selected: 0,
                        };
                    }
                }
            }
            _ => {}
        }
    }

    pub fn modal_move(&mut self, delta: i32) {
        if let Modal::KillProcess { selected, .. } = &mut self.modal {
            if delta > 0 && *selected < 1 {
                *selected += 1;
            } else if delta < 0 && *selected > 0 {
                *selected -= 1;
            }
        }
    }

    pub fn modal_confirm(&mut self) {
        if let Modal::KillProcess { pid, selected, .. } = self.modal.clone() {
            if selected == 0 {
                let _ = nix::sys::signal::kill(
                    nix::unistd::Pid::from_raw(pid as i32),
                    nix::sys::signal::Signal::SIGTERM,
                );
            }
            self.modal = Modal::None;
        }
    }

    pub fn modal_cancel(&mut self) {
        self.modal = Modal::None;
    }

    /// Compute the viewport scroll offset so that the selected item appears
    /// roughly in the middle of the visible area (only starts scrolling once
    /// the selection reaches the midpoint).
    pub fn viewport_offset(&self, visible_rows: usize) -> usize {
        if visible_rows == 0 {
            return 0;
        }
        let half = visible_rows / 2;
        if self.list_scroll <= half {
            0
        } else {
            let total = self.current_list_len();
            let max_offset = total.saturating_sub(visible_rows);
            (self.list_scroll - half).min(max_offset)
        }
    }
}
