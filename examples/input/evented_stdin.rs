// this is a hack around the fact that tokio::io::stdin() is actually
// blocking, which makes it useless for interactive programs. this isn't great
// (or particularly correct) but it mostly works.

use std::io::Read as _;

struct EventedStdin;

const STDIN: i32 = 0;

impl std::io::Read for EventedStdin {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let stdin = std::io::stdin();
        let mut stdin = stdin.lock();
        stdin.read(buf)
    }
}

impl mio::Evented for EventedStdin {
    fn register(
        &self,
        poll: &mio::Poll,
        token: mio::Token,
        interest: mio::Ready,
        opts: mio::PollOpt,
    ) -> std::io::Result<()> {
        let fd = STDIN as std::os::unix::io::RawFd;
        let eventedfd = mio::unix::EventedFd(&fd);
        eventedfd.register(poll, token, interest, opts)
    }

    fn reregister(
        &self,
        poll: &mio::Poll,
        token: mio::Token,
        interest: mio::Ready,
        opts: mio::PollOpt,
    ) -> std::io::Result<()> {
        let fd = STDIN as std::os::unix::io::RawFd;
        let eventedfd = mio::unix::EventedFd(&fd);
        eventedfd.reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &mio::Poll) -> std::io::Result<()> {
        let fd = STDIN as std::os::unix::io::RawFd;
        let eventedfd = mio::unix::EventedFd(&fd);
        eventedfd.deregister(poll)
    }
}

pub struct Stdin {
    input: tokio::reactor::PollEvented2<EventedStdin>,
}

#[allow(dead_code)]
impl Stdin {
    pub fn new() -> Self {
        Default::default()
    }
}

impl Default for Stdin {
    fn default() -> Self {
        Self {
            input: tokio::reactor::PollEvented2::new(EventedStdin),
        }
    }
}

impl std::io::Read for Stdin {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.input.read(buf)
    }
}

impl tokio::io::AsyncRead for Stdin {
    fn poll_read(
        &mut self,
        buf: &mut [u8],
    ) -> std::result::Result<futures::Async<usize>, tokio::io::Error> {
        let ready = mio::Ready::readable();
        futures::try_ready!(self.input.poll_read_ready(ready));

        let res = self.read(buf)?;
        self.input.clear_read_ready(ready)?;
        Ok(futures::Async::Ready(res))
    }
}
