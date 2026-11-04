import Zip
-- Reads a raw DEFLATE stream from the file in argv[0] (or stdin), writes decompressed bytes
-- to stdout. Exit 0 = accepted, 2 = rejected (message on stderr). Uses the VERIFIED pure-Lean
-- production decoder `Zip.Native.Inflate.inflate`; no FFI zlib is linked.
partial def readAll (h : IO.FS.Stream) (acc : ByteArray) : IO ByteArray := do
  let chunk ← h.read 65536
  if chunk.isEmpty then return acc else readAll h (acc ++ chunk)

def main (args : List String) : IO UInt32 := do
  let data ← match args with
    | [p] => IO.FS.readBinFile p
    | _ => do readAll (← IO.getStdin) ByteArray.empty
  match Zip.Native.Inflate.inflate data (1024 * 1024 * 1024) with
  | .ok out =>
    let o ← IO.getStdout
    o.write out
    o.flush
    return 0
  | .error e =>
    IO.eprintln s!"reject: {e}"
    return 2
