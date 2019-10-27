use futures::future::Future as _;
use futures::stream::Stream as _;
use std::io::Write as _;

mod input;

fn main() {
    let mut argv = std::env::args();
    argv.next().unwrap();
    let cmd = argv.next().unwrap();
    let args: Vec<_> = argv.collect();

    let process = tokio_pty_process_stream::Process::new(
        &cmd,
        &args,
        input::evented_stdin::Stdin::new(),
    );
    let process = tokio_pty_process_stream::ResizingProcess::new(process);

    let _raw = crossterm::RawScreen::into_raw_mode().unwrap();
    tokio::run(
        process
            .for_each(|ev| {
                match ev {
                    tokio_pty_process_stream::Event::CommandStart {
                        ..
                    } => {}
                    tokio_pty_process_stream::Event::Output { data } => {
                        let stdout = std::io::stdout();
                        let mut stdout = stdout.lock();
                        stdout.write_all(&data).unwrap();
                        stdout.flush().unwrap();
                    }
                    tokio_pty_process_stream::Event::CommandExit {
                        ..
                    } => {}
                    tokio_pty_process_stream::Event::Resize { .. } => {}
                }
                futures::future::ok(())
            })
            .map_err(|e| panic!(e)),
    );
}
