use std::io::Read as _;

pub struct Stdin {
    buf: Vec<u8>,
    task: futures::task::Task,
}

#[allow(dead_code)]
impl Stdin {
    pub fn new() -> Self {
        Self {
            buf: vec![],
            task: futures::task::current(),
        }
    }

    pub fn send(&mut self, buf: &[u8]) {
        self.buf.extend(buf.iter());
        self.task.notify();
    }
}

impl std::io::Read for Stdin {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let len = self.buf.len().min(buf.len());
        buf[..len].clone_from_slice(&self.buf[..len]);
        self.buf = self.buf.iter().copied().skip(len).collect();
        Ok(len)
    }
}

impl tokio::io::AsyncRead for Stdin {
    fn poll_read(
        &mut self,
        buf: &mut [u8],
    ) -> std::result::Result<futures::Async<usize>, tokio::io::Error> {
        if self.buf.is_empty() {
            return Ok(futures::Async::NotReady);
        }
        let n = self.read(buf)?;
        Ok(futures::Async::Ready(n))
    }
}
