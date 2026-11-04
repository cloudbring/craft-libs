//! `cargo run --example dump -- <input-name>` writes the named hostile/mutation stream to stdout
//! (for replaying one case against `lzinflate` or a debugger; the oracle never stores streams).
use std::io::Write;
fn main() {
    let name = std::env::args().nth(1).expect("input name, e.g. mutation/00088");
    let mut all = inflate_oracle::hostile::cases();
    all.extend(inflate_oracle::hostile::mutations(3000, 0x00D3_F1A7));
    let (_, bytes) = all.into_iter().find(|(n, _)| *n == name).expect("no such input (use the unclassified mutation/NNNNN name)");
    std::io::stdout().write_all(&bytes).expect("write");
}
