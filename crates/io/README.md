# craft-io

Bounded file and reader reads for the Craft apps. The path API opens a file once, checks the
opened handle is a regular file, and reads at most `max + 1` bytes to distinguish a file of
exactly `max` bytes from a larger one. On Unix, it opens nonblocking so a FIFO without a writer
cannot hang during `open`. Symlinks to regular files are allowed.

```rust
use craft_io::{CappedReadError, CappedReader, read_capped, read_capped_to_string};
use std::path::Path;

let bytes = read_capped(Path::new("data.bin"), 1024)?;
let text = read_capped_to_string(Path::new("config.txt"), 1024)?;
let decoded = CappedReader::new(std::io::Cursor::new(bytes), 1024).into_bytes()?;
# let _ = (text, decoded);
# Ok::<(), CappedReadError>(())
```

`read_capped_from(file, max)` accepts an existing `File` and still checks its type before
reading; the caller must ensure opening that handle did not block. `CappedReader<R>` implements
`Read` for streams, returning an I/O error if the stream exceeds its cap; `into_bytes` maps that
condition to `CappedReadError::TooLarge { limit }`. Neither reader API can guarantee that an
arbitrary source itself does not block. On Windows, path opens check the opened handle but do
not guarantee nonblocking behavior for named pipes. Invalid UTF-8 from the string API is
reported as `CappedReadError::Io` with `InvalidData` kind.
