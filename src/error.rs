/// Errors returned by the process stream.
#[derive(Debug, snafu::Snafu)]
#[snafu(visibility(pub))]
pub enum Error {
    /// failed to open a pty
    #[snafu(display("failed to open a pty: {}", source))]
    OpenPty { source: std::io::Error },

    /// failed to poll for process exit
    #[snafu(display("failed to poll for process exit: {}", source))]
    ProcessExitPoll { source: std::io::Error },

    /// failed to read from pty
    #[snafu(display("failed to read from pty: {}", source))]
    ReadPty { source: std::io::Error },

    /// failed to read from terminal
    #[snafu(display("failed to read from terminal: {}", source))]
    ReadTerminal { source: std::io::Error },

    /// failed to resize pty
    #[snafu(display("failed to resize pty: {}", source))]
    ResizePty { source: std::io::Error },

    /// failed to spawn process
    #[snafu(display("failed to spawn process for `{}`: {}", cmd, source))]
    SpawnProcess { cmd: String, source: std::io::Error },

    /// failed to write to pty
    #[snafu(display("failed to write to pty: {}", source))]
    WritePty { source: std::io::Error },
}
