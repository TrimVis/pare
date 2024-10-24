use std::io::{self, Write};

pub struct MultiWriter<W1, W2> {
    w1: W1,
    w2: W2,
}

impl<W1, W2> MultiWriter<W1, W2> {
    pub fn new(w1: W1, w2: W2) -> MultiWriter<W1, W2> {
        return MultiWriter { w1, w2 };
    }
}

impl<W1: Write, W2: Write> Write for MultiWriter<W1, W2> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let res1 = self.w1.write(buf);
        let res2 = self.w2.write(buf);

        match (res1, res2) {
            (Ok(n1), Ok(n2)) => {
                if n1 == n2 {
                    Ok(n1)
                } else {
                    Err(io::Error::new(io::ErrorKind::Other, "Write sizes differ"))
                }
            }
            (Err(e), _) | (_, Err(e)) => Err(e),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        self.w1.flush()?;
        self.w2.flush()
    }
}
