use crate::{cli::TracingArgs, state::ProcessState};

pub fn print_execve_trace(state: &ProcessState, result: i64, tracing_args: &TracingArgs) {
    // Preconditions:
    // 1. execve syscall exit, which leads to 2
    // 2. state.exec_data is Some
    let exec_data = state.exec_data.as_ref().unwrap();
    match (tracing_args.successful_only, tracing_args.decode_errno) {
        // This is very ugly, TODO: refactor
        (true, true) => {
            println!(
                "{: <1$}{pid}<{comm}>: {fname:?} {argv:?}",
                "",
                state.indent,
                pid = state.pid,
                comm = state.comm,
                fname = exec_data.filename,
                argv = exec_data.argv,
            );
        }
        (true, false) => {
            println!(
                "{: <1$}{pid}<{comm}>: {fname:?} {argv:?} = {result}",
                "",
                state.indent,
                pid = state.pid,
                comm = state.comm,
                fname = exec_data.filename,
                argv = exec_data.argv,
                result = result
            );
        }
        (false, true) => {
            if result == 0 {
                println!(
                    "{: <1$}{pid}<{comm}>: {fname:?} {argv:?}",
                    "",
                    state.indent,
                    pid = state.pid,
                    comm = state.comm,
                    fname = exec_data.filename,
                    argv = exec_data.argv,
                );
            } else {
                println!(
                    "{: <1$}{pid}<{comm}>: {fname:?} {argv:?} = {result} ({errno})",
                    "",
                    state.indent,
                    pid = state.pid,
                    comm = state.comm,
                    fname = exec_data.filename,
                    argv = exec_data.argv,
                    result = result,
                    errno = nix::errno::Errno::from_i32(-result as i32)
                );
            }
        }
        (false, false) => {
            println!(
                "{: <1$}{pid}<{comm}>: {fname:?} {argv:?} = {result}",
                "",
                state.indent,
                pid = state.pid,
                comm = state.comm,
                fname = exec_data.filename,
                argv = exec_data.argv,
                result = result
            );
        }
    }
}
