use std::{collections::HashMap, ffi::CString};

use nix::unistd::Pid;

pub struct ProcessStateStore {
    processes: HashMap<Pid, Vec<ProcessState>>,
}

pub struct ProcessState {
    pub pid: Pid,
    pub status: ProcessStatus,
    pub start_time: u64,
    pub command: Vec<CString>,
    pub preexecve: bool,
    pub exec_data: Option<ExecData>,
}

pub enum ProcessStatus {
    Running,
    Exited(i32),
}

pub struct ExecData {
    pub filename: CString,
    pub argv: Vec<CString>,
    pub envp: Vec<CString>,
}

impl ProcessStateStore {
    pub fn new() -> Self {
        Self {
            processes: HashMap::new(),
        }
    }

    pub fn insert(&mut self, state: ProcessState) {
        self.processes
            .entry(state.pid)
            .or_insert_with(Vec::new)
            .push(state);
    }

    pub fn get_current_mut(&mut self, pid: Pid) -> Option<&mut ProcessState> {
        // The last process in the vector is the current process
        // println!("Getting {pid}");
        self.processes.get_mut(&pid)?.last_mut()
    }
}
