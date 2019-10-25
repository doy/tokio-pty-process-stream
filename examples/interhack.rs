#![allow(clippy::trivial_regex)]

use futures::stream::Stream as _;
use std::io::Write as _;
use tokio::io::AsyncRead as _;

mod input;

struct Interhack {
    process: tokio_pty_process_stream::Process<input::buf::Stdin>,
    stdin: input::evented_stdin::Stdin,
    read_buf: [u8; 4096],
}

impl Interhack {
    fn new() -> Self {
        Self {
            process: tokio_pty_process_stream::Process::new(
                "nethack",
                &[],
                input::buf::Stdin::new(),
            ),
            stdin: input::evented_stdin::Stdin::new(),
            read_buf: [0; 4096],
        }
    }

    fn filter_input(&self, buf: Vec<u8>) -> Vec<u8> {
        lazy_static::lazy_static! {
            static ref RE: regex::bytes::Regex = regex::bytes::Regex::new(
                "\x05"
            ).unwrap();
        }
        if let Some(m) = RE.find(&buf) {
            let mut new: Vec<u8> = vec![];
            new.extend(buf[..m.start()].iter());
            new.extend(b"E- Elbereth\n");
            new.extend(buf[m.end()..].iter());
            new
        } else {
            buf
        }
    }

    fn filter_output(&self, buf: Vec<u8>) -> Vec<u8> {
        lazy_static::lazy_static! {
            static ref RE: regex::bytes::Regex = regex::bytes::Regex::new(
                r"Elbereth"
            ).unwrap();
        }
        if let Some(m) = RE.find(&buf) {
            let mut new: Vec<u8> = vec![];
            new.extend(buf[..m.start()].iter());
            new.extend(b"\x1b[35m");
            new.extend(buf[m.start()..m.end()].iter());
            new.extend(b"\x1b[m");
            new.extend(buf[m.end()..].iter());
            new
        } else {
            buf
        }
    }
}

#[allow(clippy::type_complexity)]
impl Interhack {
    const POLL_FNS:
        &'static [&'static dyn for<'a> Fn(
            &'a mut Self,
        )
            -> component_future::Poll<(), ()>] =
        &[&Self::poll_input, &Self::poll_process];

    fn poll_input(&mut self) -> component_future::Poll<(), ()> {
        let n = component_future::try_ready!(self
            .stdin
            .poll_read(&mut self.read_buf)
            .map_err(|e| panic!(e)));
        let input = self.filter_input(self.read_buf[..n].to_vec());
        self.process.input().send(&input);
        Ok(component_future::Async::DidWork)
    }

    fn poll_process(&mut self) -> component_future::Poll<(), ()> {
        let event = component_future::try_ready!(self
            .process
            .poll()
            .map_err(|e| panic!(e)));
        match event {
            Some(tokio_pty_process_stream::Event::Output { data }) => {
                let output = self.filter_output(data);
                let stdout = std::io::stdout();
                let mut stdout = stdout.lock();
                stdout.write_all(&output).unwrap();
                stdout.flush().unwrap();
            }
            None => return Ok(component_future::Async::Ready(())),
            _ => {}
        }
        Ok(component_future::Async::DidWork)
    }
}

impl futures::future::Future for Interhack {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> futures::Poll<Self::Item, Self::Error> {
        component_future::poll_future(self, Self::POLL_FNS)
    }
}

fn main() {
    let _raw_screen = crossterm::RawScreen::into_raw_mode().unwrap();
    tokio::run(futures::future::lazy(Interhack::new));
}
