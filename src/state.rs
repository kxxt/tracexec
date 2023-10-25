use std::{collections::HashMap, ffi::CString, path::PathBuf};

use nix::unistd::Pid;

use crate::proc::{read_argv, read_comm, Interpreter};

pub struct ProcessStateStore {
    processes: HashMap<Pid, Vec<ProcessState>>,
}

#[derive(Debug)]
pub struct ProcessState {
    pub pid: Pid,
    pub ppid: Option<Pid>,
    pub status: ProcessStatus,
    pub start_time: u64,
    pub argv: Vec<CString>,
    pub comm: String,
    pub presyscall: bool,
    pub exec_data: Option<ExecData>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProcessStatus {
    SigstopReceived,
    PtraceForkEventReceived,
    Running,
    Exited(i32),
}

#[derive(Debug)]
pub struct ExecData {
    pub filename: String,
    pub argv: Vec<String>,
    pub envp: Vec<String>,
    pub cwd: PathBuf,
    pub interpreters: Vec<Interpreter>,
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

impl ProcessState {
    pub fn new(pid: Pid, start_time: u64) -> color_eyre::Result<Self> {
        Ok(Self {
            pid,
            ppid: None,
            status: ProcessStatus::Running,
            comm: read_comm(pid)?,
            argv: read_argv(pid)?,
            start_time,
            presyscall: true,
            exec_data: None,
        })
    }
}
