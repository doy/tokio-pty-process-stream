#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::type_complexity)]

use futures::future::Future as _;
use snafu::ResultExt as _;
use std::os::unix::io::AsRawFd as _;
use tokio::io::{AsyncRead as _, AsyncWrite as _};
use tokio_pty_process::{CommandExt as _, PtyMaster as _};

const READ_BUFFER_SIZE: usize = 4 * 1024;

#[derive(Debug, snafu::Snafu)]
pub enum Error {
    #[snafu(display("failed to open a pty: {}", source))]
    OpenPty { source: std::io::Error },

    #[snafu(display("failed to poll for process exit: {}", source))]
    ProcessExitPoll { source: std::io::Error },

    #[snafu(display("failed to read from pty: {}", source))]
    ReadPty { source: std::io::Error },

    #[snafu(display("failed to read from terminal: {}", source))]
    ReadTerminal { source: std::io::Error },

    #[snafu(display("failed to resize pty: {}", source))]
    ResizePty { source: std::io::Error },

    #[snafu(display("failed to spawn process for `{}`: {}", cmd, source))]
    SpawnProcess { cmd: String, source: std::io::Error },

    #[snafu(display("failed to write to pty: {}", source))]
    WritePty { source: std::io::Error },
}

#[derive(Debug, PartialEq, Eq)]
pub enum Event {
    CommandStart(String, Vec<String>),
    Output(Vec<u8>),
    CommandExit(std::process::ExitStatus),
}

struct State {
    pty: Option<tokio_pty_process::AsyncPtyMaster>,
    process: Option<tokio_pty_process::Child>,
}

impl State {
    fn new() -> Self {
        Self {
            pty: None,
            process: None,
        }
    }

    fn pty(&self) -> &tokio_pty_process::AsyncPtyMaster {
        self.pty.as_ref().unwrap()
    }

    fn pty_mut(&mut self) -> &mut tokio_pty_process::AsyncPtyMaster {
        self.pty.as_mut().unwrap()
    }

    fn process(&mut self) -> &mut tokio_pty_process::Child {
        self.process.as_mut().unwrap()
    }
}

pub struct Process<R: tokio::io::AsyncRead> {
    state: State,
    input: R,
    input_buf: std::collections::VecDeque<u8>,
    cmd: String,
    args: Vec<String>,
    buf: [u8; READ_BUFFER_SIZE],
    started: bool,
    exited: bool,
    needs_resize: Option<(u16, u16)>,
    stdin_closed: bool,
    stdout_closed: bool,
}

impl<R: tokio::io::AsyncRead + 'static> Process<R> {
    pub fn new(cmd: &str, args: &[String], input: R) -> Self {
        Self {
            state: State::new(),
            input,
            input_buf: std::collections::VecDeque::new(),
            cmd: cmd.to_string(),
            args: args.to_vec(),
            buf: [0; READ_BUFFER_SIZE],
            started: false,
            exited: false,
            needs_resize: None,
            stdin_closed: false,
            stdout_closed: false,
        }
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        self.needs_resize = Some((rows, cols));
    }
}

impl<R: tokio::io::AsyncRead + 'static> Process<R> {
    const POLL_FNS:
        &'static [&'static dyn for<'a> Fn(
            &'a mut Self,
        )
            -> component_future::Poll<
            Option<Event>,
            Error,
        >] = &[
        // order is important here - checking command_exit first so that we
        // don't try to read from a process that has already exited, which
        // causes an error. also, poll_resize needs to happen after
        // poll_command_start, or else the pty might not be initialized.
        &Self::poll_command_start,
        &Self::poll_command_exit,
        &Self::poll_resize,
        &Self::poll_read_stdin,
        &Self::poll_write_stdin,
        &Self::poll_read_stdout,
    ];

    fn poll_resize(
        &mut self,
    ) -> component_future::Poll<Option<Event>, Error> {
        if let Some((rows, cols)) = &self.needs_resize {
            component_future::try_ready!(self
                .state
                .pty()
                .resize(*rows, *cols)
                .context(ResizePty));
            log::debug!("resize({}x{})", cols, rows);
            self.needs_resize = None;
            Ok(component_future::Async::DidWork)
        } else {
            Ok(component_future::Async::NothingToDo)
        }
    }

    fn poll_command_start(
        &mut self,
    ) -> component_future::Poll<Option<Event>, Error> {
        if self.started {
            return Ok(component_future::Async::NothingToDo);
        }

        if self.state.pty.is_none() {
            self.state.pty = Some(
                tokio_pty_process::AsyncPtyMaster::open().context(OpenPty)?,
            );
            log::debug!(
                "openpty({})",
                self.state.pty.as_ref().unwrap().as_raw_fd()
            );
        }

        if self.state.process.is_none() {
            self.state.process = Some(
                std::process::Command::new(&self.cmd)
                    .args(&self.args)
                    .spawn_pty_async(self.state.pty())
                    .context(SpawnProcess {
                        cmd: self.cmd.clone(),
                    })?,
            );
            log::debug!(
                "spawn({})",
                self.state.process.as_ref().unwrap().id()
            );
        }

        self.started = true;
        Ok(component_future::Async::Ready(Some(Event::CommandStart(
            self.cmd.clone(),
            self.args.clone(),
        ))))
    }

    fn poll_read_stdin(
        &mut self,
    ) -> component_future::Poll<Option<Event>, Error> {
        if self.exited || self.stdin_closed {
            return Ok(component_future::Async::NothingToDo);
        }

        let n = component_future::try_ready!(self
            .input
            .poll_read(&mut self.buf)
            .context(ReadTerminal));
        log::debug!("read_stdin({})", n);
        if n > 0 {
            self.input_buf.extend(self.buf[..n].iter());
        } else {
            self.input_buf.push_back(b'\x04');
            self.stdin_closed = true;
        }
        Ok(component_future::Async::DidWork)
    }

    fn poll_write_stdin(
        &mut self,
    ) -> component_future::Poll<Option<Event>, Error> {
        if self.exited || self.input_buf.is_empty() {
            return Ok(component_future::Async::NothingToDo);
        }

        let (a, b) = self.input_buf.as_slices();
        let buf = if a.is_empty() { b } else { a };
        let n = component_future::try_ready!(self
            .state
            .pty_mut()
            .poll_write(buf)
            .context(WritePty));
        log::debug!("write_stdin({})", n);
        for _ in 0..n {
            self.input_buf.pop_front();
        }
        Ok(component_future::Async::DidWork)
    }

    fn poll_read_stdout(
        &mut self,
    ) -> component_future::Poll<Option<Event>, Error> {
        match self
            .state
            .pty_mut()
            .poll_read(&mut self.buf)
            .context(ReadPty)
        {
            Ok(futures::Async::Ready(n)) => {
                log::debug!("read_stdout({})", n);
                let bytes = self.buf[..n].to_vec();
                Ok(component_future::Async::Ready(Some(Event::Output(bytes))))
            }
            Ok(futures::Async::NotReady) => {
                Ok(component_future::Async::NotReady)
            }
            Err(e) => {
                // XXX this seems to be how eof is returned, but this seems...
                // wrong? i feel like there has to be a better way to do this
                if let Error::ReadPty { source } = &e {
                    if source.kind() == std::io::ErrorKind::Other {
                        log::debug!("read_stdout(eof)");
                        self.stdout_closed = true;
                        return Ok(component_future::Async::DidWork);
                    }
                }
                Err(e)
            }
        }
    }

    fn poll_command_exit(
        &mut self,
    ) -> component_future::Poll<Option<Event>, Error> {
        if self.exited {
            return Ok(component_future::Async::Ready(None));
        }
        if !self.stdout_closed {
            return Ok(component_future::Async::NothingToDo);
        }

        let status = component_future::try_ready!(self
            .state
            .process()
            .poll()
            .context(ProcessExitPoll));
        log::debug!("exit({})", status);
        self.exited = true;
        Ok(component_future::Async::Ready(Some(Event::CommandExit(
            status,
        ))))
    }
}

#[must_use = "streams do nothing unless polled"]
impl<R: tokio::io::AsyncRead + 'static> futures::stream::Stream
    for Process<R>
{
    type Item = Event;
    type Error = Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        component_future::poll_stream(self, Self::POLL_FNS)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use futures::sink::Sink as _;
    use futures::stream::Stream as _;

    #[test]
    fn test_simple() {
        let (wres, rres) = tokio::sync::mpsc::channel(100);
        let wres2 = wres.clone();
        let mut wres = wres.wait();
        let buf = std::io::Cursor::new(b"hello world\n");
        let fut = Process::new("cat", &[], buf)
            .for_each(move |e| {
                wres.send(Ok(e)).unwrap();
                Ok(())
            })
            .map_err(|e| {
                wres2.wait().send(Err(e)).unwrap();
            });
        tokio::run(fut);

        let mut rres = rres.wait();

        let event = rres.next();
        let event = event.unwrap();
        let event = event.unwrap();
        let event = event.unwrap();
        assert_eq!(event, Event::CommandStart("cat".to_string(), vec![]));

        let mut output: Vec<u8> = vec![];
        let mut exited = false;
        for event in rres {
            assert!(!exited);
            let event = event.unwrap();
            let event = event.unwrap();
            match event {
                Event::CommandStart(..) => panic!("unexpected CommandStart"),
                Event::Output(buf) => {
                    output.extend(buf.iter());
                }
                Event::CommandExit(status) => {
                    assert!(status.success());
                    exited = true;
                }
            }
        }
        assert!(exited);
        assert_eq!(output, b"hello world\r\nhello world\r\n");
    }
}
