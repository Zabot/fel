use std::io::{self, LineWriter, Stderr, Write};

use indicatif::MultiProgress;
use tracing_subscriber::fmt::MakeWriter;

pub struct ProgressWriter<T: Write> {
    progress: Option<MultiProgress>,
    inner: T,
}

impl<T: Write> Write for ProgressWriter<T> {
    // Is this an issue if we write when we change the number of progress bars
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self.progress.as_ref() {
            None => self.inner.write(buf),
            Some(progress) => progress.suspend(|| self.inner.write(buf)),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.progress.as_ref() {
            None => self.inner.flush(),
            Some(progress) => progress.suspend(|| self.inner.flush()),
        }
    }
}

/// ProgressTracing is a global multi progress that also supports
/// `tracing_subscriber::fmt::MakeWriter` so the progess can be used
/// without fighting with the tracing output.
#[derive(Default, Clone)]
pub struct ProgressTracing {
    pub progress: MultiProgress,
}

impl MakeWriter<'_> for ProgressTracing {
    type Writer = LineWriter<ProgressWriter<Stderr>>;

    fn make_writer(&'_ self) -> Self::Writer {
        let writer = ProgressWriter {
            progress: Some(self.progress.clone()),
            inner: io::stderr(),
        };
        LineWriter::new(writer)
    }
}
