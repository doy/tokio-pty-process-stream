use futures::future::Future as _;
use futures::stream::Stream as _;
use snafu::futures01::StreamExt as _;

/// A wrapper around `Process` which listens for terminal resize signals and
/// propagates the changes into the process running on the pty.
///
/// This is useful for running subprocesses in a pty that will ultimately be
/// displayed in the user's terminal directly.
pub struct ResizingProcess<R: tokio::io::AsyncRead + 'static> {
    process: crate::process::Process<R>,
    resizer: Box<
        dyn futures::stream::Stream<
                Item = (u16, u16),
                Error = crate::error::Error,
            > + Send,
    >,
}

impl<R: tokio::io::AsyncRead + 'static> ResizingProcess<R> {
    /// Creates a new `ResizingProcess` as a wrapper around the given
    /// `Process` instance.
    pub fn new(process: crate::process::Process<R>) -> Self {
        Self {
            process,
            resizer: Box::new(
                tokio_terminal_resize::resizes()
                    .flatten_stream()
                    .context(crate::error::Resize),
            ),
        }
    }

    /// Returns a mutable reference to the input object provided in the inner
    /// `Process` instance's constructor.
    pub fn input(&mut self) -> &mut R {
        self.process.input()
    }
}

impl<R: tokio::io::AsyncRead + 'static> ResizingProcess<R> {
    const POLL_FNS:
        &'static [&'static dyn for<'a> Fn(
            &'a mut Self,
        )
            -> component_future::Poll<
            Option<crate::process::Event>,
            crate::error::Error,
        >] = &[&Self::poll_resize, &Self::poll_process];

    fn poll_resize(
        &mut self,
    ) -> component_future::Poll<
        Option<crate::process::Event>,
        crate::error::Error,
    > {
        let (rows, cols) =
            component_future::try_ready!(self.resizer.poll()).unwrap();
        self.process.resize(rows, cols);
        Ok(component_future::Async::Ready(Some(
            crate::process::Event::Resize { size: (rows, cols) },
        )))
    }

    fn poll_process(
        &mut self,
    ) -> component_future::Poll<
        Option<crate::process::Event>,
        crate::error::Error,
    > {
        Ok(component_future::Async::Ready(
            component_future::try_ready!(self.process.poll()),
        ))
    }
}

#[must_use = "streams do nothing unless polled"]
impl<R: tokio::io::AsyncRead + 'static> futures::stream::Stream
    for ResizingProcess<R>
{
    type Item = crate::process::Event;
    type Error = crate::error::Error;

    fn poll(&mut self) -> futures::Poll<Option<Self::Item>, Self::Error> {
        component_future::poll_stream(self, Self::POLL_FNS)
    }
}
