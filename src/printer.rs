use std::io::{stdout, Write};

use crate::{cli::TracingArgs, state::ProcessState};

pub fn print_execve_trace(
    state: &ProcessState,
    result: i64,
    tracing_args: &TracingArgs,
) -> color_eyre::Result<()> {
    // Preconditions:
    // 1. execve syscall exit, which leads to 2
    // 2. state.exec_data is Some
    let exec_data = state.exec_data.as_ref().unwrap();
    let mut stdout = stdout();
    write!(stdout, "{}", state.pid)?;
    let trace_comm = !tracing_args.no_trace_comm;
    let trace_argv = !tracing_args.no_trace_argv;
    let trace_env = tracing_args.trace_env;
    let trace_filename = !tracing_args.no_trace_filename;
    if trace_comm {
        write!(stdout, "<{}>", state.comm)?;
    }
    write!(stdout, ":")?;
    if trace_filename {
        write!(stdout, " {:?}", exec_data.filename)?;
    }
    if trace_argv {
        write!(stdout, " {:?}", exec_data.argv)?;
    }
    if trace_env {
        write!(stdout, " {:?}", exec_data.envp)?;
    }
    if result == 0 {
        writeln!(stdout)?;
    } else {
        let decode_errno = !tracing_args.no_decode_errno;
        if decode_errno {
            writeln!(
                stdout,
                " = {} ({})",
                result,
                nix::errno::Errno::from_i32(-result as i32)
            )?;
        } else {
            writeln!(stdout, " = {} ", result)?;
        }
    }
    Ok(())
}
